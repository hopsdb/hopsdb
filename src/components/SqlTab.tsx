import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import CodeMirror, { type ReactCodeMirrorRef } from "@uiw/react-codemirror";
import { EditorView, dropCursor } from "@codemirror/view";
import { autocompletion, type Completion, type CompletionContext } from "@codemirror/autocomplete";
import { sql } from "@codemirror/lang-sql";
import Resizer from "./Resizer";
import {
  createColumnHelper,
  flexRender,
  getCoreRowModel,
  getSortedRowModel,
  useReactTable,
  type SortingState,
} from "@tanstack/react-table";
import { explainPlan, exportCsv, exportExcel, formatSql, getColumnList, getTableList, runSql } from "../api";
import type { ColumnInfo, QueryResult, TableInfo } from "../types";

interface Props {
  sessionId: string;
  /** Initial SQL content (empty for new tabs, prefilled for source tabs) */
  initialContent?: string;
  /** Push editor changes back to the parent tab store */
  onContentChange?: (content: string) => void;
  /** Read-only (viewing PACKAGE source) */
  readOnly?: boolean;
  /** Update mode (allows INSERT/UPDATE/DELETE) */
  updateMode: boolean;
  /** Database flavor for query templates and completion */
  dbType: string;
  /** Notify the parent of busy state changes */
  onBusyChange?: (busy: boolean) => void;
  /** Register action handlers in the parent titlebar */
  onRegisterActions?: (actions: {
    run: () => void;
    plan: () => void;
    format: () => void;
  } | null) => void;
  /** Global running lock shared across SQL tabs */
  globalBusy?: boolean;
  /** Whether this tab is the currently visible/active one. Only the active
   * tab responds to Ctrl+Enter / Ctrl+E / Ctrl+Q (otherwise every mounted tab
   * would run simultaneously on those hotkeys). */
  active?: boolean;
  /** SQL environment ready (initSql finished). Completion metadata (tables,
   * columns) is only fetched once ready — backends aren't ready during the
   * brief window after mount, so a naive on-mount fetch returns empty. */
  sqlReady?: boolean;
  /** Opaque counter bumped by the parent to request this tab re-focus its
   * editor (e.g. after loading tables or clicking a toolbar button). */
  focusTick?: number;
  /** Show an error */
  onError?: (msg: string) => void;
}

/** SQL tab — editor, result table, and text output */
export default function SqlTab({
  sessionId,
  initialContent = "",
  onContentChange,
  readOnly = false,
  updateMode,
  dbType,
  onBusyChange,
  onRegisterActions,
  globalBusy = false,
  active = true,
  sqlReady = true,
  focusTick = 0,
  onError,
}: Props) {
  const [sqlText, setSqlText] = useState(initialContent);
  const [result, setResult] = useState<QueryResult | null>(null);
  const [textOut, setTextOut] = useState<string[]>([]);
  /** True once the user has executed a statement. Before that (initial tab)
   * the empty result area shows explanatory notes; after running, it only
   * shows SQL client results (even if empty). */
  const [hasRun, setHasRun] = useState(false);
  const [busy, setBusy] = useState(false);
  const [menu, setMenu] = useState<{ x: number; y: number } | null>(null);
  const [columnInfos, setColumnInfos] = useState<ColumnInfo[]>([]);
  const [tableInfos, setTableInfos] = useState<TableInfo[]>([]);
  const [editorHeight, setEditorHeight] = useState(260);
  const editorStartRef = useRef(260);
  const editorRef = useRef<ReactCodeMirrorRef>(null);
  const busyRef = useRef(false);
  /** Last cursor offset, restored when this tab becomes active. */
  const cursorPosRef = useRef<number | null>(null);

  const setBusyState = useCallback(
    (b: boolean) => {
      busyRef.current = b;
      setBusy(b);
      onBusyChange?.(b);
    },
    [onBusyChange],
  );

  useEffect(() => {
    setSqlText(initialContent);
  }, [initialContent]);

  useEffect(() => {
    let alive = true;
    // Wait for the SQL environment to be ready before fetching completion
    // metadata. The initial tab mounts during connection, before initSql has
    // finished — a naive fetch there returns empty completion data.
    if (!sqlReady) return () => undefined;
    (async () => {
      try {
        const cols = await getColumnList(sessionId, dbType);
        // Structured { name, comment } — autocomplete shows comment as detail.
        if (alive) setColumnInfos(cols);
      } catch {
        if (alive) setColumnInfos([]);
      }
      try {
        const tables = await getTableList(sessionId, dbType);
        // getTableList returns structured { name, comment }. Keep the full list
        // so autocomplete can show the comment as a `detail` (not inserted).
        if (alive) setTableInfos(tables);
      } catch {
        if (alive) setTableInfos([]);
      }
    })();
    return () => {
      alive = false;
    };
  }, [dbType, sessionId, sqlReady]);

  const dbCompletions = useMemo<Completion[]>(() => {
    const base: Completion[] = [
      { label: "select", type: "keyword" },
      { label: "from", type: "keyword" },
      { label: "where", type: "keyword" },
      { label: "order by", type: "keyword" },
      { label: "group by", type: "keyword" },
      { label: "count(*)", type: "function" },
    ];

    const db = dbType.toLowerCase();
    const dbSpecific: Record<string, Completion[]> = {
      oracle: [
        { label: "dual", type: "table" },
        { label: "rownum", type: "variable" },
        { label: "sysdate", type: "constant" },
        { label: "decode()", type: "function" },
      ],
      mysql: [
        { label: "limit", type: "keyword" },
        { label: "now()", type: "function" },
        { label: "ifnull()", type: "function" },
        { label: "information_schema", type: "table" },
      ],
      postgresql: [
        { label: "limit", type: "keyword" },
        { label: "current_timestamp", type: "constant" },
        { label: "coalesce()", type: "function" },
        { label: "pg_catalog", type: "table" },
      ],
      generic: [{ label: "limit", type: "keyword" }],
    };

    return [...base, ...(dbSpecific[db] ?? dbSpecific.generic)];
  }, [dbType]);

  const rowLimitHint = useMemo(() => {
    const db = dbType.toLowerCase();
    if (db === "mysql" || db === "postgresql" || db === "postgres") {
      return (
        <>
          statements are automatically limited with <span className="mono">LIMIT 1000</span>{" "}
          (unless you already specify <span className="mono">LIMIT</span> yourself).
        </>
      );
    }
    return (
      <>
        statements are automatically limited to <span className="mono">ROWNUM &le; 1000</span>{" "}
        (unless you already specify <span className="mono">ROWNUM</span> yourself).
      </>
    );
  }, [dbType]);

  const completionSource = useCallback(
    (context: CompletionContext) => {
      const word = context.matchBefore(/[\w$#]+/);
      if (!word && !context.explicit) return null;

      const options: Completion[] = [
        ...dbCompletions,
        ...tableInfos.map((t) => ({
          label: t.name,
          type: "table" as const,
          // Gray secondary text (not inserted). Mirrors the object tree's
          // comment display: visible, distinct style, excluded on insert.
          ...(t.comment ? { detail: t.comment } : {}),
        })),
        ...columnInfos.map((c) => ({
          label: c.name,
          type: "property" as const,
          ...(c.comment ? { detail: c.comment } : {}),
        })),
      ];

      return {
        from: word ? word.from : context.pos,
        options,
        validFor: /^[\w$#]*$/,
      };
    },
    [columnInfos, tableInfos, dbCompletions],
  );

  // Selected text, otherwise the full text
  const getTargetSql = useCallback((): string => {
    const view = editorRef.current?.view;
    if (view) {
      const { from, to } = view.state.selection.main;
      if (from !== to) {
        return view.state.sliceDoc(from, to);
      }
    }
    return sqlText;
  }, [sqlText]);

  /** Run SQL */
  const onRun = useCallback(async () => {
    if (busyRef.current || globalBusy) return;
    console.info("[hopsdb] sql run requested", { busy: busyRef.current, globalBusy, readOnly });
    let sql = getTargetSql()?.trim();
    if (!sql) return;

    // Any statement that modifies data/objects requires Update Mode — not just
    // UPDATE/DELETE, but CREATE/DROP/ALTER/TRUNCATE and friends too. Match the
    // leading verb (after optional comments / whitespace / parentheses) so a
    // SELECT containing such a word elsewhere isn't blocked.
    const updateVerbs = [
      "INSERT",
      "UPDATE",
      "DELETE",
      "MERGE",
      "REPLACE",
      "CREATE",
      "DROP",
      "ALTER",
      "TRUNCATE",
      "RENAME",
      "GRANT",
      "REVOKE",
      "PURGE",
      "COMMENT",
    ];
    // Strip comments, then read the first meaningful word.
    const stripped = sql.replace(/\/\*[\s\S]*?\*\//g, "").replace(/--[^\n]*/g, "").trim();
    const firstWord = (stripped.match(/^[\s(]*([a-zA-Z]+)/) ?? [])[1]?.toUpperCase() ?? "";
    const isUpdate = updateVerbs.includes(firstWord);
    if (!updateMode && isUpdate) {
      onError?.("This operation modifies data (enable Update Mode to run it)");
      return;
    }

    setBusyState(true);
    setHasRun(true);
    setResult(null);
    setTextOut([]);
    try {
      const r = await runSql(sessionId, sql, dbType);
      if (r.columns.length > 0) {
        setResult(r);
      } else if (r.text.length > 0) {
        setTextOut(r.text);
      }
    } catch (e) {
      onError?.(e instanceof Error ? e.message : String(e));
    } finally {
      setBusyState(false);
      // Return keyboard focus to the editor after running.
      editorRef.current?.view?.focus();
    }
  }, [dbType, getTargetSql, globalBusy, onError, readOnly, sessionId, setBusyState, updateMode]);

  /** Explain Plan */
  const onPlan = useCallback(async () => {
    if (busyRef.current || globalBusy) return;
    console.info("[hopsdb] sql plan requested", { busy: busyRef.current, globalBusy, readOnly });
    let sql = getTargetSql()?.trim();
    if (!sql) return;
    setBusyState(true);
    setHasRun(true);
    setResult(null);
    setTextOut([]);
    try {
      const r = await explainPlan(sessionId, sql, dbType);
      if (r.columns.length > 0) {
        setResult(r);
      } else if (r.text.length > 0) {
        setTextOut(r.text);
      }
    } catch (e) {
      onError?.(e instanceof Error ? e.message : String(e));
    } finally {
      setBusyState(false);
      // Return keyboard focus to the editor after planning.
      editorRef.current?.view?.focus();
    }
  }, [dbType, getTargetSql, globalBusy, onError, readOnly, sessionId, setBusyState]);

  /** Format SQL */
  const onFormat = useCallback(() => {
    if (busyRef.current || globalBusy) return;
    console.info("[hopsdb] sql format requested", { busy: busyRef.current, globalBusy, readOnly });
    try {
      setSqlText(formatSql(sqlText));
      // Return keyboard focus to the editor after formatting.
      editorRef.current?.view?.focus();
    } catch {
      // Keep the original text if formatting fails
    }
  }, [globalBusy, readOnly, sqlText]);

  /** Export to Excel */
  const onExport = useCallback(async () => {
    if (!result || result.columns.length === 0) return;
    try {
      const info = await exportExcel(result.columns, result.rows);
      onError?.(`Exported: ${info.file_path} (${info.rows} rows × ${info.cols} cols)`);
    } catch (e) {
      onError?.(e instanceof Error ? e.message : String(e));
    }
  }, [result, onError]);

  /** Export to CSV (saved to the user's Downloads folder) */
  const onExportCsv = useCallback(async () => {
    if (!result || result.columns.length === 0) return;
    try {
      const info = await exportCsv(result.columns, result.rows);
      onError?.(`Exported CSV: ${info.file_path} (${info.rows} rows × ${info.cols} cols)`);
    } catch (e) {
      onError?.(e instanceof Error ? e.message : String(e));
    }
  }, [result, onError]);

  useEffect(() => {
    if (readOnly) {
      onRegisterActions?.(null);
      console.info("[hopsdb] sql actions cleared", { readOnly, dbType });
      return;
    }
    console.info("[hopsdb] sql actions registered", { readOnly, dbType });
    onRegisterActions?.({
      run: onRun,
      plan: onPlan,
      format: onFormat,
    });
    return () => onRegisterActions?.(null);
  }, [onRegisterActions, onRun, onPlan, onFormat, readOnly]);

  // Shortcuts: Ctrl+Enter Run / Ctrl+E Plan / Ctrl+Q Format (active tab only,
  // so an inactive mounted tab doesn't also run on the same hotkey).
  useEffect(() => {
    if (!active) return;
    const handler = (e: KeyboardEvent) => {
      if (readOnly) return;
      const mod = e.ctrlKey || e.metaKey;
      if (mod && e.key === "Enter") {
        e.preventDefault();
        onRun();
      } else if (mod && e.key.toLowerCase() === "e") {
        e.preventDefault();
        onPlan();
      } else if (mod && e.key.toLowerCase() === "q") {
        e.preventDefault();
        onFormat();
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [onRun, onPlan, onFormat, readOnly, active]);

  // Focus the editor + restore the cursor when this tab becomes active, so the
  // user can type immediately without clicking into the pane. Also re-runs when
  // the parent bumps `focusTick` (e.g. after tables load or a toolbar click) to
  // return keyboard focus to the SQL editor. Read-only (source) tabs stay put.
  useEffect(() => {
    if (!active || readOnly) return;
    let cancelled = false;
    const doFocus = () => {
      if (cancelled) return;
      const view = editorRef.current?.view;
      if (!view) return;
      view.focus();
      const pos =
        cursorPosRef.current != null ? cursorPosRef.current : view.state.doc.length;
      view.dispatch({
        selection: { anchor: pos },
        scrollIntoView: true,
      });
    };
    // The ref may not be populated on first render (tab just mounted), so retry
    // shortly to make sure the editor is ready before focusing.
    doFocus();
    if (!editorRef.current?.view) {
      const t = setTimeout(doFocus, 60);
      return () => {
        cancelled = true;
        clearTimeout(t);
      };
    }
  }, [active, focusTick, readOnly]);

  // Remember the last cursor position in this tab so it can be restored when
  // the tab is re-activated.
  const cursorTrackExt = useMemo(
    () =>
      EditorView.updateListener.of((upd) => {
        if (upd.view.hasFocus) {
          cursorPosRef.current = upd.view.state.selection.main.anchor;
        }
      }),
    [],
  );

  // Drop handler: allow dragging a tree node name / selected text from outside
  // into the SQL editor. Non-JS insertion for read-only tabs is ignored.
  const dropExt = useMemo(
    () =>
      readOnly
        ? []
        : [
            dropCursor(),
            EditorView.domEventHandlers({
              drop(event, view) {
                if (event.dataTransfer) {
                  const text = event.dataTransfer.getData("text/plain");
                  if (text) {
                    const pos = view.posAtCoords({ x: event.clientX, y: event.clientY });
                    if (pos != null) {
                      event.preventDefault();
                      view.dispatch({
                        changes: { from: pos, insert: text },
                        selection: { anchor: pos + text.length },
                      });
                    }
                  }
                }
              },
            }),
          ],
    [readOnly],
  );

  // Fill the whole editor box with the content surface so even an empty editor
  // accepts clicks / context-menu / caret placement anywhere (not just on text).
  const fillExt = useMemo(
    () =>
      EditorView.theme({
        "&": { height: "100%" },
        ".cm-scroller": { overflow: "auto" },
        ".cm-content": { flex: "1 1 auto", paddingBottom: "2em" },
      }),
    [],
  );

  return (
    <div
      className={`sql-tab ${readOnly ? "is-readonly" : ""}`}
      onClick={() => setMenu(null)}
      onContextMenu={(e) => {
        e.preventDefault();
        // Don't clear the menu here — right-clicking the editor must open it.
        // Only block the browser default and stop bubbling into MainWindow.
        e.stopPropagation();
      }}
    >
      <div className="sql-editor-wrap" style={{ height: editorHeight }}>
        <CodeMirror
          ref={editorRef}
          value={sqlText}
          height="100%"
          theme="dark"
          editable={!readOnly && !globalBusy}
          extensions={[
            sql(),
            fillExt,
            // Autocomplete only on editable (new SQL) tabs.
            ...(readOnly
              ? []
              : [autocompletion({ override: [completionSource] })]),
            cursorTrackExt,
            ...dropExt,
          ]}
          onChange={(val) => {
            setSqlText(val);
            onContentChange?.(val);
          }}
          placeholder="Enter SQL, Ctrl+Enter to run…"
          onContextMenu={(e) => {
            e.preventDefault();
            e.stopPropagation();
            if (!readOnly) {
              setMenu({ x: e.clientX, y: e.clientY });
            }
          }}
          basicSetup={{
            lineNumbers: true,
            highlightActiveLine: true,
            foldGutter: true,
          }}
        />
        {busy && <div className="sql-busy">Running…</div>}
      </div>

      {!readOnly && (
        <>
          <Resizer
            direction="h"
            onDragStart={() => { editorStartRef.current = editorHeight; }}
            onDrag={(d) => setEditorHeight(Math.min(600, Math.max(80, editorStartRef.current + d)))}
          />
          <div className="sql-result-area">
            {result && result.columns.length > 0 ? (
              <ResultTable columns={result.columns} rows={result.rows} onExport={onExport} onExportCsv={onExportCsv} />
            ) : textOut.length > 0 ? (
              <pre className="sql-text-out">{textOut.join("\n")}</pre>
            ) : !hasRun ? (
              <div className="sql-hint">
                <div className="sql-hint-title">SQL Console Notes</div>
                <ul>
                  <li>
                    To avoid transferring huge datasets, <span className="mono">SELECT</span>{" "}
                    {rowLimitHint}
                  </li>
                  <li>
                    The editor is <strong>read-only by default</strong> to prevent accidental
                    changes. Toggle <strong>Update Mode</strong> (top-right) to allow{" "}
                    <span className="mono">INSERT / UPDATE / DELETE</span>.
                  </li>
                  <li>
                    This console behaves like a direct session. After{" "}
                    <span className="mono">INSERT / UPDATE / DELETE</span> you must explicitly run{" "}
                    <span className="mono">COMMIT</span> or <span className="mono">ROLLBACK</span> —
                    that depends on your client&apos;s autocommit setting. Oracle init turns it{" "}
                    <span className="mono">off</span>; mysql/psql keep their session defaults unless
                    your command set changes them.
                  </li>
                  <li>
                    This tool is a thin terminal over your database session. You are solely
                    responsible for the consequences of every statement you execute, including any
                    data loss from <span className="mono">INSERT / UPDATE / DELETE</span> or{" "}
                    <span className="mono">COMMIT</span>.
                  </li>
                </ul>
              </div>
            ) : (
              <div className="sql-result-empty">No results</div>
            )}
          </div>
        </>
      )}

      {menu && !readOnly && (
        <div className="sql-context-menu" style={{ left: menu.x, top: menu.y }} onClick={(e) => e.stopPropagation()}>
          <button type="button" onClick={onRun}>
            Execute
          </button>
          <button type="button" onClick={onPlan}>
            Explain Plan
          </button>
          <button type="button" onClick={onFormat}>
            Format
          </button>
          <button type="button" onClick={onExport} disabled={!result || result.columns.length === 0}>
            Export Excel
          </button>
          <button type="button" onClick={onExportCsv} disabled={!result || result.columns.length === 0}>
            Export CSV
          </button>
        </div>
      )}
    </div>
  );
}

/** Result table with column sorting */
function ResultTable({
  columns,
  rows,
  onExport,
  onExportCsv,
}: {
  columns: string[];
  rows: string[][];
  onExport: () => void;
  onExportCsv: () => void;
}) {
  const [sorting, setSorting] = useState<SortingState>([]);
  const colHelper = createColumnHelper<Record<string, string>>();

  const columnsDef = useMemo(
    () =>
      columns.map((c, i) =>
        colHelper.accessor((row) => row[`c${i}`] ?? "", {
          id: `c${i}`,
          header: c,
          cell: (info) => info.getValue(),
          enableSorting: true,
          size: 180,
          minSize: 80,
          maxSize: 800,
        }),
      ),
    [columns, colHelper],
  );

  const data = useMemo(
    () =>
      rows.map((r) => {
        const obj: Record<string, string> = {};
        r.forEach((v, i) => {
          obj[`c${i}`] = v;
        });
        return obj;
      }),
    [rows],
  );

  const table = useReactTable({
    data,
    columns: columnsDef,
    state: { sorting },
    onSortingChange: setSorting,
    enableColumnResizing: true,
    columnResizeMode: "onChange",
    getCoreRowModel: getCoreRowModel(),
    getSortedRowModel: getSortedRowModel(),
  });

  return (
    <div className="result-table-wrap">
      <div className="result-toolbar">
        <span className="result-count">
          {data.length} rows × {columns.length} cols
        </span>
        <div className="result-toolbar-actions">
          <button className="export-btn" onClick={onExport}>
            <span className="export-ico">📊</span> Export Excel
          </button>
          <button className="export-btn" onClick={onExportCsv}>
            <span className="export-ico">📄</span> Export CSV
          </button>
        </div>
      </div>
      <div className="result-table-scroll">
        <table className="result-table">
          <thead>
            {table.getHeaderGroups().map((hg) => (
              <tr key={hg.id}>
                {hg.headers.map((h) => (
                  <th
                    key={h.id}
                    onClick={h.column.getToggleSortingHandler()}
                    title={h.column.getCanSort() ? "Click to sort" : undefined}
                    style={{ width: h.getSize() }}
                  >
                    {flexRender(h.column.columnDef.header, h.getContext())}
                    {h.column.getIsSorted() === "asc" && " ▲"}
                    {h.column.getIsSorted() === "desc" && " ▼"}
                    <div
                      aria-label={`Resize ${String(h.column.columnDef.header)} column`}
                      className={`result-column-resizer ${h.column.getIsResizing() ? "is-resizing" : ""}`}
                      data-testid={`column-resizer-${h.column.id}`}
                      onClick={(e) => e.stopPropagation()}
                      onMouseDown={h.getResizeHandler()}
                      onTouchStart={h.getResizeHandler()}
                      onDoubleClick={(e) => {
                        e.stopPropagation();
                        h.column.resetSize();
                      }}
                      role="separator"
                    />
                  </th>
                ))}
              </tr>
            ))}
          </thead>
          <tbody>
            {table.getRowModel().rows.map((row) => (
              <tr key={row.id}>
                {row.getVisibleCells().map((cell) => (
                  <td key={cell.id} style={{ width: cell.column.getSize() }}>
                    {flexRender(cell.column.columnDef.cell, cell.getContext())}
                  </td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  );
}
