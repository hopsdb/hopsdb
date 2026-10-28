import { useCallback, useEffect, useMemo, useState } from "react";
import type { ChangeEvent } from "react";
import { connect, isTauriRuntime } from "../api";
import type { CommandStep, SshConnection } from "../types";

interface CommandPassword {
  lineIndex: number;
  password: string;
}

/** Saved command-set profile */
export interface Connection {
  id: string;
  label: string;
  sshUsername: string;
  sshHostname: string;
  sshPort: number;
  sshPassword: string;
  commands: string;
  commandPasswords: CommandPassword[];
}

type LegacyConnection = Partial<Connection> & {
  user?: string;
  host?: string;
  password?: string;
  sshUsername?: string;
  sshHostname?: string;
  sshPort?: number;
  sshPassword?: string;
  commandPasswords?: Partial<CommandPassword>[];
};

type ConnectionForm = Omit<Connection, "id">;

const CONNECTIONS_KEY = "hopsdb.connections";

function loadConnections(): Connection[] {
  try {
    const parsed = JSON.parse(localStorage.getItem(CONNECTIONS_KEY) ?? "[]") as LegacyConnection[];
    return parsed.map(normalizeStoredConnection);
  } catch {
    return [];
  }
}

function normalizeStoredConnection(c: LegacyConnection, index: number): Connection {
  const migrated = migrateLegacySsh(c);
  const originalPasswords = normalizeCommandPasswords(
    c.commandPasswords ?? [],
    (c.commands ?? "").split("\n").length,
  );
  const sshPassword = c.sshPassword ?? c.password ?? migrated.sshPassword ?? "";

  return {
    id: c.id ?? `conn-${Date.now()}-${index}`,
    label: c.label ?? "",
    sshUsername: c.sshUsername ?? migrated.username,
    sshHostname: c.sshHostname ?? migrated.hostname,
    sshPort: c.sshPort ?? migrated.port,
    sshPassword,
    commands: migrated.commands,
    commandPasswords: migrated.commandPasswords(originalPasswords),
  };
}

function migrateLegacySsh(c: LegacyConnection) {
  const lines = (c.commands ?? "").split("\n");
  const firstIndex = firstNonEmptyLineIndex(c.commands ?? "");
  const parsed = firstIndex >= 0 ? parseLegacySshLine(lines[firstIndex] ?? "") : null;
  const removeFirstSsh = parsed !== null;
  const commands = removeFirstSsh
    ? lines.filter((_, index) => index !== firstIndex).join("\n").trim()
    : c.commands ?? "";
  const legacyHost = splitLegacyHost(c.host ?? "");
  const username = c.user?.trim() || parsed?.username || "";
  const hostname = legacyHost.hostname || parsed?.hostname || "";
  const port = parsed?.port ?? legacyHost.port;
  return {
    commands,
    username,
    hostname,
    port,
    sshPassword: removeFirstSsh
      ? (c.commandPasswords ?? []).find((item) => item.lineIndex === firstIndex)?.password
      : undefined,
    commandPasswords: (passwords: CommandPassword[]) =>
      removeFirstSsh
        ? passwords
            .filter((item) => item.lineIndex !== firstIndex)
            .map((item) => ({
              ...item,
              lineIndex: item.lineIndex > firstIndex ? item.lineIndex - 1 : item.lineIndex,
            }))
        : passwords,
  };
}

function splitLegacyHost(host: string): { hostname: string; port: number } {
  const match = /^(.+):(\d+)$/.exec(host.trim());
  return match
    ? { hostname: match[1] ?? "", port: Number(match[2]) }
    : { hostname: host.trim(), port: 22 };
}

function parseLegacySshLine(line: string): { username: string; hostname: string; port: number } | null {
  const match = /^\s*ssh\s+(?:-p\s+(\d+)\s+)?([^@\s]+)@([^\s]+)\s*$/.exec(line);
  if (!match) return null;
  return { username: match[2] ?? "", hostname: match[3] ?? "", port: Number(match[1] ?? 22) };
}

function normalizeCommandPasswords(
  values: Partial<CommandPassword>[],
  lineCount: number,
): CommandPassword[] {
  const byLine = new Map<number, CommandPassword>();
  for (const value of values) {
    if (
      typeof value.lineIndex === "number" &&
      value.lineIndex >= 0 &&
      value.lineIndex < lineCount &&
      typeof value.password === "string" &&
      value.password.length > 0
    ) {
      byLine.set(value.lineIndex, {
        lineIndex: value.lineIndex,
        password: value.password,
      });
    }
  }
  return [...byLine.values()].sort((a, b) => a.lineIndex - b.lineIndex);
}

const emptyForm = (): ConnectionForm => ({
  label: "",
  sshUsername: "",
  sshHostname: "",
  sshPort: 22,
  sshPassword: "",
  commands: "",
  commandPasswords: [],
});

function firstNonEmptyCommand(commands: string): string {
  return commands
    .split("\n")
    .map((line) => line.trim())
    .find((line) => line.length > 0) ?? "";
}

function firstNonEmptyLineIndex(commands: string): number {
  return commands.split("\n").findIndex((line) => line.trim().length > 0);
}

function commandSummary(commands: string): string {
  const first = firstNonEmptyCommand(commands);
  return first || "No commands";
}

function buildCommandSteps(form: ConnectionForm): CommandStep[] {
  const passwords = new Map(form.commandPasswords.map((item) => [item.lineIndex, item.password]));
  return form.commands
    .split("\n")
    .map((line, lineIndex) => {
      const command = line.trim();
      if (!command) return null;
      const password = passwords.get(lineIndex);
      return password ? { command, password } : { command };
    })
    .filter((step): step is CommandStep => step !== null);
}

function buildSshConnection(form: ConnectionForm): SshConnection {
  return {
    username: form.sshUsername.trim(),
    hostname: form.sshHostname.trim(),
    port: form.sshPort,
    ...(form.sshPassword ? { password: form.sshPassword } : {}),
  };
}

interface Props {
  onConnected: (
    sessionId: string,
    userHost: string,
    dbType: string | null,
    hasSqlClient: boolean,
    transcript: string[],
  ) => void;
}

/** Connection manager — stores command-set profiles and per-line passwords. */
export default function ConnectionManager({ onConnected }: Props) {
  const [connections, setConnections] = useState<Connection[]>(() => loadConnections());
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [form, setForm] = useState<ConnectionForm>(emptyForm());
  const [isNew, setIsNew] = useState(false);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);

  const selected = useMemo(
    () => connections.find((c) => c.id === selectedId) ?? null,
    [connections, selectedId],
  );

  const commandRows = useMemo(
    () =>
      form.commands
        .split("\n")
        .map((line, lineIndex) => ({ lineIndex, command: line.trim() }))
        .filter((row) => row.command.length > 0),
    [form.commands],
  );

  // Persist whenever the list changes
  useEffect(() => {
    localStorage.setItem(CONNECTIONS_KEY, JSON.stringify(connections));
  }, [connections]);

  const persist = useCallback((list: Connection[]) => {
    setConnections(list);
    localStorage.setItem(CONNECTIONS_KEY, JSON.stringify(list));
  }, []);

  const cleanForm = useCallback((): ConnectionForm => {
    if (!form.label.trim()) {
      throw new Error("Label is required");
    }
    if (!form.sshUsername.trim()) {
      throw new Error("SSH username is required");
    }
    if (!form.sshHostname.trim()) {
      throw new Error("SSH hostname is required");
    }
    if (!Number.isInteger(form.sshPort) || form.sshPort < 1 || form.sshPort > 65535) {
      throw new Error("SSH port must be between 1 and 65535");
    }
    return {
      ...form,
      label: form.label.trim(),
      sshUsername: form.sshUsername.trim(),
      sshHostname: form.sshHostname.trim(),
      commands: form.commands.trimEnd(),
      commandPasswords: normalizeCommandPasswords(
        form.commandPasswords,
        form.commands.split("\n").length,
      ),
    };
  }, [form]);

  const saveCurrentForm = useCallback((): string => {
    const cleaned = cleanForm();
    if (!selectedId) {
      const id = `conn-${Date.now()}`;
      persist([...connections, { ...cleaned, id }]);
      setSelectedId(id);
      setIsNew(false);
      setForm(cleaned);
      return id;
    }

    persist(
      connections.map((c) =>
        c.id === selectedId ? { ...cleaned, id: selectedId } : c,
      ),
    );
    setForm(cleaned);
    return selectedId;
  }, [cleanForm, selectedId, connections, persist]);

  /** Start a new command-set form */
  const onNew = useCallback(() => {
    setForm(emptyForm());
    setIsNew(true);
    setSelectedId(null);
    setError("");
  }, []);

  /** Select an existing command set and load it into the form */
  const onSelect = useCallback((c: Connection) => {
    setSelectedId(c.id);
    setIsNew(false);
    setForm({
      label: c.label,
      sshUsername: c.sshUsername,
      sshHostname: c.sshHostname,
      sshPort: c.sshPort,
      sshPassword: c.sshPassword,
      commands: c.commands,
      commandPasswords: c.commandPasswords,
    });
    setError("");
  }, []);

  /** Save the current form (create or update) */
  const onSave = useCallback(() => {
    try {
      saveCurrentForm();
      setError("");
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  }, [saveCurrentForm]);

  /** Delete the selected command set */
  const onDelete = useCallback(() => {
    if (!selectedId) return;
    if (!window.confirm(`Delete connection "${selected?.label}"?`)) return;
    persist(connections.filter((c) => c.id !== selectedId));
    setSelectedId(null);
    setIsNew(false);
    setForm(emptyForm());
    setError("");
  }, [selectedId, selected, connections, persist]);

  /** Connect using the current command set (auto-saves first) */
  const onConnect = useCallback(async () => {
    setBusy(true);
    setError("");
    try {
      const cleaned = cleanForm();
      saveCurrentForm();
      const steps = buildCommandSteps(cleaned);
      const res = await connect(buildSshConnection(cleaned), steps);
      onConnected(
        res.session_id,
        res.user_host,
        res.sql_client_db_type,
        res.startup_complete && res.sql_client_db_type !== null,
        res.transcript,
      );
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(false);
    }
  }, [cleanForm, onConnected, saveCurrentForm]);

  const setText = (field: "label" | "commands" | "sshUsername" | "sshHostname" | "sshPassword") => (e: ChangeEvent<HTMLInputElement | HTMLTextAreaElement>) =>
    setForm((prev) => ({ ...prev, [field]: e.target.value }));

  const passwordForLine = (lineIndex: number) =>
    form.commandPasswords.find((item) => item.lineIndex === lineIndex)?.password ?? "";

  const hasPasswordForLine = (lineIndex: number) =>
    form.commandPasswords.some((item) => item.lineIndex === lineIndex);

  const setLinePasswordEnabled = (lineIndex: number, enabled: boolean) => {
    setForm((prev) => {
      const existingPassword =
        prev.commandPasswords.find((item) => item.lineIndex === lineIndex)?.password ?? "";
      const withoutLine = prev.commandPasswords.filter((item) => item.lineIndex !== lineIndex);
      if (!enabled) {
        return { ...prev, commandPasswords: withoutLine };
      }
      return {
        ...prev,
        commandPasswords: [...withoutLine, { lineIndex, password: existingPassword }]
          .sort((a, b) => a.lineIndex - b.lineIndex),
      };
    });
  };

  const setLinePassword = (lineIndex: number, password: string) => {
    setForm((prev) => {
      const withoutLine = prev.commandPasswords.filter((item) => item.lineIndex !== lineIndex);
      return {
        ...prev,
        commandPasswords: [...withoutLine, { lineIndex, password }]
          .sort((a, b) => a.lineIndex - b.lineIndex),
      };
    });
  };

  return (
    <div className="login-screen">
      <div className="conn-manager">
        <img className="conn-manager-logo" src="/favicon.png" alt="HopsDB" />
        {/* Left: connection list */}
        <aside className="conn-list">
          <div className="conn-list-header">
            <span>Connections</span>
            <button className="btn-small" onClick={onNew} title="New connection" disabled={busy}>
              + New
            </button>
          </div>
          <div className="conn-list-body">
            {connections.length === 0 ? (
              <div className="conn-empty">
                No connections yet.
                <br />
                Create your first one.
              </div>
            ) : (
              connections.map((c) => (
                <div
                  key={c.id}
                  className={`conn-item ${c.id === selectedId ? "active" : ""}`}
                  onClick={() => {
                    if (!busy) onSelect(c);
                  }}
                >
                  <div className="conn-item-label-row">
                    <div className="conn-item-label">{c.label || commandSummary(c.commands)}</div>
                  </div>
                  <div className="conn-item-sub">{commandSummary(c.commands)}</div>
                </div>
              ))
            )}
          </div>
        </aside>

        {/* Right: edit form */}
        {selected || isNew ? (
          <form
            className={`conn-form ${busy ? "is-busy" : ""}`}
            onSubmit={(e) => {
              e.preventDefault();
              onConnect();
            }}
          >
            <div className="conn-form-title">
              {isNew ? "New Command Set" : selected?.label}
              {busy && (
                <span className="conn-loading-chip" aria-live="polite">
                  <span className="conn-loading-spinner" />
                  Connecting
                </span>
              )}
            </div>

            {!isTauriRuntime && !busy && (
              <div className="login-error">
                Desktop app only: connection and database commands work inside the Tauri app, not
                in a plain browser.
              </div>
            )}

            {error && (
              <div className="login-error conn-error-pop" aria-live="polite">
                <span className="conn-error-title">Connection failed</span>
                <span className="conn-error-body">{error}</span>
              </div>
            )}

            <fieldset className="conn-form-fields" disabled={busy || !isTauriRuntime}>
              <label className="field">
                <span>Label</span>
                <input value={form.label} onChange={setText("label")} placeholder="e.g. Local Oracle" />
              </label>

              <div className="conn-ssh-fields">
                <label className="field">
                  <span>SSH Username</span>
                  <input value={form.sshUsername} onChange={setText("sshUsername")} autoComplete="username" />
                </label>
                <label className="field">
                  <span>SSH Hostname</span>
                  <input value={form.sshHostname} onChange={setText("sshHostname")} placeholder="host or IP" />
                </label>
                <label className="field">
                  <span>SSH Port</span>
                  <input
                    type="number"
                    min={1}
                    max={65535}
                    value={form.sshPort}
                    onChange={(e) => setForm((prev) => ({ ...prev, sshPort: Number(e.target.value) }))}
                  />
                </label>
                <label className="field">
                  <span>SSH Password <small>(optional)</small></span>
                  <input type="password" value={form.sshPassword} onChange={setText("sshPassword")} autoComplete="current-password" />
                </label>
              </div>

              <label className="field command-set-field">
                <span>Command set (Run after ssh login, For MySQL/MariaDB MUST add --html option)</span>
                <textarea
                  value={form.commands}
                  onChange={setText("commands")}
                  rows={6}
                  placeholder={
                    "docker exec -it oracle bash\n" +
                    "export NLS_LANG=AMERICAN_AMERICA.AL32UTF8\n" +
                    "export ORACLE_HOME=/opt/oracle/product/26ai/dbhomeFree\n" +
                    "export PATH=$ORACLE_HOME/bin:$PATH\n" +
                    "sqlplus system/password@localhost:1521/FREEPDB1\n\n" +
                    "# MySQL example: mysql -u app -p shop\n" +
                    "# PostgreSQL example: psql -U app -d shop"
                  }
                  className="conn-commands"
                />
                <p className="command-set-note">
                  SQL view opens only when HopsDB detects a default database prompt: <code>SQL&gt;</code>, <code>mysql&gt;</code>, <code>MariaDB [db]&gt;</code>, or <code>dbname=&gt;</code> / <code>dbname=#</code>. Do not customize the client prompt (for example <code>NEWSQL&gt;</code>), otherwise HopsDB cannot identify the database type or open SQL view automatically.
                </p>
              </label>

              <div className="command-password-panel">
                <div className="command-password-header">
                  <div>
                    <span>Line passwords</span>
                    <p>Generated from the command set above. Edit commands there; use Pwd for lines that may ask for a password.</p>
                  </div>
                  <span>{commandRows.length} commands</span>
                </div>
                {commandRows.length === 0 ? (
                  <div className="command-password-empty">No command lines</div>
                ) : (
                  <>
                    <div className="command-password-columns" aria-hidden="true">
                      <span>Use</span>
                      <span>Command preview (read only)</span>
                      <span>Password</span>
                    </div>
                    {commandRows.map((row) => {
                      const enabled = hasPasswordForLine(row.lineIndex);
                      return (
                        <div className="command-password-row" key={row.lineIndex}>
                          <button
                            type="button"
                            className={`line-password-toggle ${enabled ? "active" : ""}`}
                            onClick={() => setLinePasswordEnabled(row.lineIndex, !enabled)}
                            title={`Toggle password for line ${row.lineIndex + 1}`}
                          >
                            {enabled ? "Pwd" : "Off"}
                          </button>
                          <div className="line-command-preview" title={row.command}>
                            <span className="line-number">{row.lineIndex + 1}</span>
                            <code>{row.command}</code>
                          </div>
                          {enabled && (
                            <input
                              type="password"
                              value={passwordForLine(row.lineIndex)}
                              onChange={(e) => setLinePassword(row.lineIndex, e.target.value)}
                              placeholder={`Password for line ${row.lineIndex + 1}`}
                              aria-label={`Password for command line ${row.lineIndex + 1}`}
                            />
                          )}
                        </div>
                      );
                    })}
                  </>
                )}
              </div>
            </fieldset>

            <div className="conn-form-actions">
              <button
                type="submit"
                className={`btn-primary ${busy ? "is-busy" : ""}`}
                disabled={busy || !isTauriRuntime}
              >
                {busy ? "Connecting..." : "Connect"}
              </button>
              <button type="button" className="btn-small" onClick={onSave} disabled={busy || !isTauriRuntime}>
                Save
              </button>
              <button
                type="button"
                className="btn-small btn-danger"
                onClick={onDelete}
                disabled={busy || !selectedId || isNew || !isTauriRuntime}
              >
                Delete
              </button>
            </div>
          </form>
        ) : (
          <div className="conn-form conn-form-empty">
            <div className="conn-form-empty-title">No connection selected</div>
            <div className="conn-form-empty-text">
              Pick a saved connection, or click + New to create one.
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
