import { useEffect, useState } from "react";
import ConnectionManager from "./components/ConnectionManager";
import MainWindow from "./components/MainWindow";
import "./App.css";

type ThemePreset =
  | "midnight"
  | "ocean"
  | "forest"
  | "ember"
  | "violet"
  | "sand"
  | "mint"
  | "graphite"
  | "sunrise"
  | "noir";

const THEME_KEY = "hopsdb.theme";

const THEME_LABELS: Record<ThemePreset, string> = {
  midnight: "Midnight",
  ocean: "Ocean",
  forest: "Forest",
  ember: "Ember",
  violet: "Violet",
  sand: "Sand",
  mint: "Mint",
  graphite: "Graphite",
  sunrise: "Sunrise",
  noir: "Noir",
};

const KNOWN_THEMES = new Set<ThemePreset>(Object.keys(THEME_LABELS) as ThemePreset[]);

type SessionItem =
  | { id: string; kind: "login"; title: string }
  | {
      id: string;
      kind: "workspace";
      title: string;
      sessionId: string;
      userHost: string;
      dbType: string | null;
      initialView: "sql" | "shell";
      shellHistorySeed: string[];
    };

const LOGIN_SESSION_ID = "login";

function App() {
  const [sessions, setSessions] = useState<SessionItem[]>(() => [{ id: LOGIN_SESSION_ID, kind: "login", title: "Login" }]);
  const [activeSessionId, setActiveSessionId] = useState<string>(LOGIN_SESSION_ID);
  const [theme, setTheme] = useState<ThemePreset>(() => {
    const saved = localStorage.getItem(THEME_KEY);
    return KNOWN_THEMES.has(saved as ThemePreset) ? (saved as ThemePreset) : "midnight";
  });

  useEffect(() => {
    document.documentElement.dataset.theme = theme;
    localStorage.setItem(THEME_KEY, theme);
  }, [theme]);

  const renderThemePicker = () => (
    <label className="theme-switcher" aria-label="Theme preset">
      <span className="theme-switcher-label">Theme</span>
      <select
        className="theme-switcher-select"
        value={theme}
        onChange={(e) => setTheme(e.target.value as ThemePreset)}
      >
        {Object.entries(THEME_LABELS).map(([value, label]) => (
          <option key={value} value={value}>
            {label}
          </option>
        ))}
      </select>
    </label>
  );

  const handleConnected = (
    sessionId: string,
    userHost: string,
    dbType: string | null,
    hasSqlClient: boolean,
    transcript: string[],
  ) => {
    const id = `ws-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
    setSessions((prev) => [
      ...prev,
      {
        id,
        kind: "workspace",
        title: userHost,
        sessionId,
        userHost,
        dbType,
        initialView: hasSqlClient ? "sql" : "shell",
        shellHistorySeed: transcript,
      },
    ]);
    setActiveSessionId(id);
  };

  const handleDbTypeDetected = (workspaceId: string, dbType: string) => {
    setSessions((prev) =>
      prev.map((session) =>
        session.id === workspaceId && session.kind === "workspace"
          ? { ...session, dbType, initialView: "sql" }
          : session,
      ),
    );
  };

  const activeSession = sessions.find((session) => session.id === activeSessionId);

  return (
    <div className="workspace-shell">
      <aside className="session-tabs">
        {sessions.map((session) => (
          <button
            key={session.id}
            className={`session-tab ${session.id === activeSessionId ? "active" : ""} ${session.kind}`}
            onClick={() => setActiveSessionId(session.id)}
            type="button"
          >
            <span className="session-tab-title">{session.title}</span>
          </button>
        ))}
      </aside>

      <main className="session-content">
        {activeSession?.kind === "login" && <div className="session-topbar">{renderThemePicker()}</div>}
        <div className="session-pane">
          {sessions.map((session) => (
            <div
              key={session.id}
              className="session-instance"
              style={{ display: session.id === activeSessionId ? "flex" : "none" }}
            >
              {session.kind === "login" ? (
                <ConnectionManager onConnected={handleConnected} />
              ) : (
                <MainWindow
                  sessionId={session.sessionId}
                  userHost={session.userHost}
                  dbType={session.dbType}
                  initialView={session.initialView}
                  initialShellHistory={session.shellHistorySeed}
                  themePicker={renderThemePicker()}
                  onDbTypeDetected={(dbType) => handleDbTypeDetected(session.id, dbType)}
                  onDisconnected={() => {
                    void import("./api").then(({ disconnect }) => disconnect(session.sessionId).catch(() => {}));
                    setSessions((prev) => prev.filter((s) => s.id !== session.id));
                    setActiveSessionId(LOGIN_SESSION_ID);
                  }}
                />
              )}
            </div>
          ))}
        </div>
      </main>
    </div>
  );
}

export default App;
