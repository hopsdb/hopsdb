import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type ReactNode,
} from "react";
import ObjectTree from "./ObjectTree";
import SqlTab from "./SqlTab";
import ShellTab from "./ShellTab";
import Resizer from "./Resizer";
import { disconnect, getObjectSource, initSql, setUpdateMode as callSetUpdateMode } from "../api";
import type { TabItem } from "../types";

interface Props {
  sessionId: string;
  userHost: string;
  dbType: string | null;
  initialView: "sql" | "shell";
  initialShellHistory: string[];
  themePicker: ReactNode;
  onDbTypeDetected: (dbType: string) => void;
  onDisconnected: () => void;
}

let tabSeq = 0;
const newTabId = () => `tab-${++tabSeq}`;
const initialTabs = [
  { id: newTabId(), kind: "sql" as const, title: "Sql-0", content: "" },
];

/** Main window — object tree and tabbed SQL / Command panes */
export default function MainWindow({
  sessionId,
  userHost,
  dbType,
  initialView,
  initialShellHistory,
  themePicker,
  onDbTypeDetected,
  onDisconnected,
}: Props) {
  const [tabs, setTabs] = useState<TabItem[]>(() => initialTabs);
  const [activeTab, setActiveTab] = useState<string>(() => initialTabs[0].id);
  const [viewMode, setViewMode] = useState<"sql" | "shell">(() => initialView);
  const [updateMode, setUpdateMode] = useState(false);
  const [sqlReady, setSqlReady] = useState(false);
  const [shellHistory, setShellHistory] = useState<string[]>(() => initialShellHistory);
  const [runningCount, setRunningCount] = useState(0);
  const running = runningCount > 0;
  const [status, setStatus] = useState(`Connected to ${userHost}`);
  const [toast, setToast] = useState("");
  const [sessionKey] = useState(() => String(Date.now()));
  const [treeVersion, setTreeVersion] = useState(0);
  // 每次递增都让活动的 SQL tab 重新聚焦到编辑器（load tables / 按钮点击后）。
  const [focusTick, setFocusTick] = useState(0);
  const refocusEditor = useCallback(() => setFocusTick((t) => t + 1), []);
  const [sidebarWidth, setSidebarWidth] = useState(240);
  const sidebarStartRef = useRef(240);
  const handleSidebarDragStart = useCallback(() => {
    sidebarStartRef.current = sidebarWidth;
  }, [sidebarWidth]);
  const handleSidebarDrag = useCallback((d: number) => {
    setSidebarWidth(Math.min(600, Math.max(150, sidebarStartRef.current + d)));
  }, []);
  const [tabMenu, setTabMenu] = useState<{ x: number; y: number; tabId: string } | null>(null);
  const [tabActions, setTabActions] = useState<{
    run: () => void;
    plan: () => void;
    format: () => void;
  } | null>(null);
  const toastTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const tabBarRef = useRef<HTMLDivElement | null>(null);
  const sqlInitRef = useRef<{ key: string; promise: Promise<string> } | null>(null);

  const showToast = useCallback((msg: string) => {
    setToast(msg);
    if (toastTimer.current) clearTimeout(toastTimer.current);
    toastTimer.current = setTimeout(() => setToast(""), 3500);
  }, []);

  const onError = useCallback(
    (msg: string) => {
      showToast(msg);
    },
    [showToast],
  );

  const setBusyFlag = useCallback((busy: boolean) => {
    console.info("[hopsdb] busy flag", busy);
    setRunningCount((prev) => {
      const next = busy ? prev + 1 : Math.max(0, prev - 1);
      return next;
    });
  }, []);

  useEffect(() => {
    setStatus(running ? "Running…" : `Connected to ${userHost}`);
  }, [running, userHost]);

  /** New SQL tab */
  const addSqlTab = useCallback((initialContent = "") => {
    const id = newTabId();
    setTabs((prev) => [
      ...prev,
      { id, kind: "sql", title: `Sql-${prev.length}`, content: initialContent },
    ]);
    setActiveTab(id);
    return id;
  }, []);

  const updateTabContent = useCallback((id: string, content: string) => {
    setTabs((prev) => prev.map((tab) => (tab.id === id ? { ...tab, content } : tab)));
  }, []);

  // Initialize the SQL environment only once we are in SQL view.
  useEffect(() => {
    if (viewMode !== "sql" || !dbType) return;
    let alive = true;
    const key = `${sessionId}\u0000${dbType}`;
    let initialization = sqlInitRef.current;
    if (!initialization || initialization.key !== key) {
      console.info("[hopsdb] initSql begin", { userHost, dbType });
      initialization = { key, promise: initSql(sessionId, dbType) };
      sqlInitRef.current = initialization;
    }
    (async () => {
      try {
        await initialization.promise;
        if (!alive) return;
        setSqlReady(true);
        console.info("[hopsdb] initSql success", { userHost, dbType });
        setStatus(`Connected to ${userHost} · SQL environment ready`);
      } catch (error) {
        if (!alive) return;
        console.info("[hopsdb] initSql failed", { userHost, dbType });
        const message = error instanceof Error ? error.message : String(error);
        setStatus(`Connected to ${userHost} · SQL environment unavailable`);
        showToast(message);
      }
    })();
    return () => {
      alive = false;
    };
  }, [sessionId, userHost, dbType, viewMode, showToast]);

  useEffect(() => {
    setViewMode(initialView);
  }, [initialView, userHost]);

  useEffect(() => {
    if (initialShellHistory.length === 0) return;
    setShellHistory((prev) => (prev.length > 0 ? prev : initialShellHistory));
  }, [initialShellHistory, setShellHistory]);

  useEffect(() => {
    if (sqlReady) {
      console.info("[hopsdb] sqlReady true -> remount tree", { userHost, dbType });
      setTreeVersion((prev) => prev + 1);
    }
  }, [sqlReady]);

  /** Open source tab (double-click PACKAGE) */
  const openSource = useCallback(
    async (objType: string, name: string) => {
      if (!dbType) return;
      setBusyFlag(true);
      try {
        const lines = await getObjectSource(sessionId, objType, name, dbType);
        const id = newTabId();
        setTabs((prev) => [
          ...prev,
          { id, kind: "source", title: name, content: lines.join("\n") },
        ]);
        setActiveTab(id);
      } catch (e) {
        onError(e instanceof Error ? e.message : String(e));
      } finally {
        setBusyFlag(false);
      }
    },
    [sessionId, dbType, onError, setBusyFlag],
  );

  const handleSqlPromptDetected = useCallback((detectedDbType: string) => {
    onDbTypeDetected(detectedDbType);
    setViewMode("sql");
    refocusEditor();
  }, [onDbTypeDetected, refocusEditor]);

  const appendShellHistory = useCallback((lines: string[]) => {
    setShellHistory((prev) => [...prev, ...lines]);
  }, []);

  /** Close tab. Any SQL tab can be closed down to zero (the "+ SQL" button
   * is always available to create a new one). */
  const closeTab = useCallback(
    (id: string) => {
      setTabs((prev) => {
        const idx = prev.findIndex((t) => t.id === id);
        if (idx === -1) return prev;
        const next = [...prev];
        next.splice(idx, 1);
        if (activeTab === id) {
          setActiveTab(next[Math.min(idx, next.length - 1)]?.id ?? "");
        }
        return next;
      });
    },
    [activeTab],
  );

  /** Close all tabs except the given one. Unlike closeTab, this does its own
   * guard so at least one SQL tab always remains. */
  const closeOthers = useCallback(
    (id: string) => {
      setTabs((prev) => {
        const keep = prev.find((t) => t.id === id) ?? prev[0];
        if (!keep) return prev;
        // Keep only the target — SQL tabs can all be closed; "+ SQL" stays.
        const next = [keep];
        if (!next.some((t) => t.id === activeTab)) {
          setActiveTab(next[0]?.id ?? "");
        }
        return next;
      });
      setTabMenu(null);
    },
    [activeTab],
  );

  /** Close all tabs to the right of the given one. */
  const closeToRight = useCallback(
    (id: string) => {
      setTabs((prev) => {
        const idx = prev.findIndex((t) => t.id === id);
        if (idx === -1) return prev;
        const next = prev.slice(0, idx + 1);
        if (!next.some((t) => t.id === activeTab)) {
          setActiveTab(next[next.length - 1]?.id ?? "");
        }
        return next;
      });
      setTabMenu(null);
    },
    [activeTab],
  );

  /** Disconnect */
  const onDisconnect = useCallback(async () => {
    try {
      await disconnect(sessionId);
    } catch {
      // Ignore disconnect errors
    }
    onDisconnected();
  }, [onDisconnected, sessionId]);

  const activeItem = tabs.find((t) => t.id === activeTab);

  // When the active tab changes (new tab, switching, opening source), scroll the
  // tab bar so the newly-active tab is visible.
  useEffect(() => {
    if (!activeTab) return;
    const bar = tabBarRef.current;
    if (!bar) return;
    const el = bar.querySelector(`[data-tabid="${activeTab}"]`);
    if (el) {
      (el as HTMLElement).scrollIntoView({ behavior: "smooth", inline: "nearest", block: "nearest" });
    }
  }, [activeTab, tabs.length]);

  useEffect(() => {
    console.info("[hopsdb] main window state", {
      userHost,
      dbType,
      sqlReady,
      runningCount,
      running,
      activeTab,
      activeKind: activeItem?.kind,
      hasTabActions: !!tabActions,
    });
  }, [userHost, dbType, sqlReady, runningCount, running, activeTab, activeItem?.kind, tabActions]);

  return (
    <div
      className="main-window"
      onClick={() => setTabMenu(null)}
      onContextMenu={(e) => {
        e.preventDefault();
        setTabMenu(null);
      }}
    >
      {/* Title bar */}
      <header className="titlebar">
        <div className="titlebar-title">
          <img className="titlebar-logo" src="/favicon.png" alt="" aria-hidden="true" />
          HopsDB <span className="titlebar-sub">{userHost}</span>
        </div>
        <div className="titlebar-actions">
          <div className="titlebar-theme-slot">{themePicker}</div>
          <button
            className="titlebar-btn"
            onClick={() => {
            tabActions?.run();
            refocusEditor();
          }}
            disabled={!sqlReady || running || !tabActions}
            title="Run selected SQL"
          >
            <span className="tb-ico">▶</span> Run
          </button>
          <button
            className="titlebar-btn"
            onClick={() => {
            tabActions?.plan();
            refocusEditor();
          }}
            disabled={!sqlReady || running || !tabActions}
            title="Explain execution plan"
          >
            <span className="tb-ico">🔍</span> Plan
          </button>
          <button
            className="titlebar-btn"
            onClick={() => {
            tabActions?.format();
            refocusEditor();
          }}
            disabled={!sqlReady || running || !tabActions}
            title="Format SQL"
          >
            <span className="tb-ico">✦</span> Format
          </button>
          <button
            className={`titlebar-btn ${sqlReady ? "" : "btn-disabled"}`}
            onClick={() => {
            addSqlTab();
            refocusEditor();
          }}
            disabled={!sqlReady}
            title={sqlReady ? "New SQL tab" : "Waiting for SQL environment…"}
          >
            <span className="tb-ico">＋</span> SQL
          </button>
          <label className="toggle">
            <input
              type="checkbox"
              checked={updateMode}
              onChange={(e) => {
                const next = e.target.checked;
                setUpdateMode(next);
                // Sync to the backend guard too — otherwise the frontend would
                // allow the edit but the backend run_sql still blocks it.
                callSetUpdateMode(sessionId, next).catch(() => {});
                refocusEditor();
              }}
            />
            <span>Update Mode</span>
          </label>
          <button className="titlebar-btn danger" onClick={onDisconnect} title="Disconnect from server">
            <span className="tb-ico">⏻</span> Disconnect
          </button>
        </div>
      </header>

      {/* Body: object tree + tab area */}
      <div className="main-body">
        {viewMode !== "shell" && dbType && (
          <>
            <aside className="sidebar" style={{ width: sidebarWidth }}>
              <ObjectTree
                key={`${sessionKey}-${treeVersion}`}
                sessionKey={sessionKey}
                sessionId={sessionId}
                dbType={dbType}
                globalBusy={running}
                autoLoadTables={sqlReady}
                onBusyChange={setBusyFlag}
                onDoubleClick={openSource}
                onTablesLoaded={refocusEditor}
              />
            </aside>
            <Resizer
              direction="v"
              onDragStart={handleSidebarDragStart}
              onDrag={handleSidebarDrag}
            />
          </>
        )}

        <section className="tab-area">
          {viewMode === "shell" ? (
            <ShellTab
              sessionId={sessionId}
              history={shellHistory}
              onAppendHistory={appendShellHistory}
              onSqlPromptDetected={handleSqlPromptDetected}
            />
          ) : dbType ? (
            <>
              <div className="tab-bar" ref={tabBarRef}>
                {tabs.map((t) => (
                  <div
                    key={t.id}
                    data-tabid={t.id}
                    className={`tab-item ${t.id === activeTab ? "active" : ""} ${t.kind === "source" ? "src-tab" : ""}`}
                    onClick={() => setActiveTab(t.id)}
                    onAuxClick={(e) => {
                      if (e.button === 1) closeTab(t.id); // middle-click closes
                    }}
                    onContextMenu={(e) => {
                      e.preventDefault();
                      e.stopPropagation();
                      setTabMenu({ x: e.clientX, y: e.clientY, tabId: t.id });
                    }}
                  >
                    <span className={`tab-icon ${t.kind}`}>
                      {t.kind === "source" ? "{}" : "▶"}
                    </span>
                    {t.title}
                    <span
                      className="tab-close"
                      onClick={(e) => {
                        e.stopPropagation();
                        closeTab(t.id);
                      }}
                      title="Close"
                    >
                      ×
                    </span>
                  </div>
                ))}
              </div>

              <div className="tab-content">
                {tabs.length === 0 ? (
                  <div className="tab-empty">
                    No tabs open. Use <strong>+ SQL</strong> to open a new editor.
                  </div>
                ) : (
                  tabs.map((t) => {
                    const isActive = t.id === activeTab;
                    const registerActions =
                      isActive && t.kind !== "source"
                        ? setTabActions
                        : undefined;
                    if (t.kind === "source") {
                      return (
                        <div key={t.id} className="tab-pane" style={{ display: isActive ? "flex" : "none" }}>
                          <SqlTab
                            sessionId={sessionId}
                            key={t.id}
                            initialContent={t.content}
                            onContentChange={(content) => updateTabContent(t.id, content)}
                            active={isActive}
                            readOnly
                            updateMode={updateMode}
                            dbType={dbType}
                            sqlReady={sqlReady}
                            focusTick={focusTick}
                            globalBusy={running}
                            onError={onError}
                          />
                        </div>
                      );
                    }
                    if (t.kind === "sql") {
                      return (
                        <div key={t.id} className="tab-pane" style={{ display: isActive ? "flex" : "none" }}>
                          <SqlTab
                            sessionId={sessionId}
                            key={t.id}
                            initialContent={t.content}
                            onContentChange={(content) => updateTabContent(t.id, content)}
                            active={isActive}
                            onRegisterActions={registerActions}
                            updateMode={updateMode}
                            dbType={dbType}
                            sqlReady={sqlReady}
                            focusTick={focusTick}
                            globalBusy={running}
                            onBusyChange={setBusyFlag}
                            onError={onError}
                          />
                        </div>
                      );
                    }
                    return null;
                  })
                )}
              </div>
            </>
          ) : (
            <div className="shell-placeholder">Waiting for a recognized database prompt.</div>
          )}
        </section>
      </div>

      {tabMenu && (
        <div
          className="tab-context-menu"
          style={{ left: tabMenu.x, top: tabMenu.y }}
          onClick={(e) => e.stopPropagation()}
        >
          <button
            type="button"
            onClick={() => {
              const target = tabs.find((t) => t.id === tabMenu.tabId);
              if (target?.kind === "sql" || target?.kind === "source") {
                const content = target.content ?? "";
                const newId = addSqlTab(content);
                setActiveTab(newId);
              }
              setTabMenu(null);
            }}
          >
            New Tab
          </button>
          <button
            type="button"
            onClick={() => {
              closeTab(tabMenu.tabId);
              setTabMenu(null);
            }}
          >
            Close
          </button>
          <button
            type="button"
            onClick={() => closeOthers(tabMenu.tabId)}
            disabled={tabs.length <= 1}
          >
            Close Others
          </button>
          <button
            type="button"
            onClick={() => closeToRight(tabMenu.tabId)}
            disabled={tabMenu.tabId === tabs[tabs.length - 1]?.id}
          >
            Close Tabs to the Right
          </button>
        </div>
      )}

      {/* Status bar */}
      <footer className="statusbar">
        <span>{status}</span>
        <span className="statusbar-right">
          {updateMode ? "Update Mode ON" : "Read-only"} · Ctrl+Enter Run · Ctrl+E Plan · Ctrl+Q Format
        </span>
      </footer>

      {toast && <div className="toast">{toast}</div>}
    </div>
  );
}
