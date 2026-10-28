import { useEffect, useRef, useState } from "react";
import { execCommand } from "../api";

const SESSION_CLOSED_MESSAGE = "Session closed. Reconnect to run more commands.";

/** Shell tab — command input and output transcript */
interface Props {
  sessionId: string;
  history: string[];
  onAppendHistory: (lines: string[]) => void;
  onSqlPromptDetected: (dbType: string) => void;
}

export default function ShellTab({ sessionId, history, onAppendHistory, onSqlPromptDetected }: Props) {
  const [cmd, setCmd] = useState("");
  const [busy, setBusy] = useState(false);
  const [closed, setClosed] = useState(false);
  const outputRef = useRef<HTMLDivElement>(null);

  const send = async (
    command: string,
  ) => {
    if (!command.trim() || busy || closed) return;
    setBusy(true);
    onAppendHistory([`$ ${command}`]);
    try {
      const result = await execCommand(sessionId, command);
      const lines = result.lines;
      const outputLines = lines.length === 0 ? ["(no output)"] : lines;
      onAppendHistory(outputLines);
      if (outputLines.includes(SESSION_CLOSED_MESSAGE)) {
        setClosed(true);
      }
      if (result.sql_client_db_type) {
        onSqlPromptDetected(result.sql_client_db_type);
      }
    } catch (e) {
      const message = e instanceof Error ? e.message : String(e);
      onAppendHistory([`Error: ${message}`]);
      if (message.includes("SSH shell is closed")) {
        setClosed(true);
      }
    } finally {
      setBusy(false);
      setCmd("");
    }
  };

  const lineClassName = (line: string) => {
    if (line.startsWith("$ ")) return "shell-cmd";
    if (line.startsWith("[waiting]")) return "shell-waiting";
    if (line.startsWith("Error:")) return "shell-error";
    return "shell-line";
  };

  useEffect(() => {
    // Keep the viewport on the latest command output.
    outputRef.current?.scrollTo({ top: outputRef.current.scrollHeight });
  }, [history]);

  useEffect(() => {
    setClosed(false);
  }, [sessionId]);

  return (
    <div className="shell-tab">
      <div className="shell-output" ref={outputRef}>
        {history.length === 0 ? (
          <div className="shell-placeholder">
            Type a command and press Enter. If a sqlplus, mysql, or psql prompt is detected,
            the SQL view opens automatically.
          </div>
        ) : (
          history.map((line, i) => (
            <div key={i} className={lineClassName(line)}>
              {line}
            </div>
          ))
        )}
      </div>
      <div className="shell-input-row">
        <span className="shell-prompt">$</span>
        <input
          className="shell-input"
          value={cmd}
          onChange={(e) => setCmd(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") send(cmd);
          }}
          placeholder={closed ? "Session closed" : busy ? "Running…" : "Type a command"}
          disabled={busy || closed}
          autoFocus
        />
        <button className="btn-small" onClick={() => send(cmd)} disabled={busy || closed}>
          Send
        </button>
      </div>
    </div>
  );
}
