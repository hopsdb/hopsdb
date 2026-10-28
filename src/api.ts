import { invoke } from "@tauri-apps/api/core";
import type {
  ApiResult,
  CommandStep,
  SshConnection,
  ColumnInfo,
  ExportInfo,
  QueryResult,
  TableInfo,
  TreeNodeType,
} from "./types";

/** True when the app is running inside the Tauri desktop shell */
export const isTauriRuntime =
  typeof window !== "undefined" &&
  (("__TAURI_INTERNALS__" in window && Boolean((window as any).__TAURI_INTERNALS__)) ||
    ("__TAURI__" in window && Boolean((window as any).__TAURI__)));

/** Handle ApiResult returned by invoke */
async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (!isTauriRuntime) {
    throw new Error("This action is only available in the Tauri desktop app.");
  }
  const res = await invoke<ApiResult<T>>(cmd, args);
  if (!res.ok) {
    throw new Error(res.error ?? "Unknown error");
  }
  return res.data as T;
}

/** Connect to the first SSH hop, then execute the raw command-set lines. */
export function connect(
  ssh: SshConnection,
  commandSteps: CommandStep[],
): Promise<{
  session_id: string;
  user_host: string;
  message: string;
  transcript: string[];
  startup_complete: boolean;
  sql_client_ready: boolean;
  sql_client_db_type: string | null;
}> {
  return call<{
    session_id: string;
    user_host: string;
    message: string;
    transcript: string[];
    startup_complete: boolean;
    sql_client_ready: boolean;
    sql_client_db_type: string | null;
  }>("connect", {
    ssh,
    commandSteps,
  });
}

/** Disconnect */
export function disconnect(sessionId: string): Promise<string> {
  return call<string>("disconnect", { sessionId });
}

/** Execute a shell command */
export function execCommand(
  sessionId: string,
  command: string,
): Promise<{ lines: string[]; sql_client_db_type: string | null }> {
  return call<{ lines: string[]; sql_client_db_type: string | null }>("exec_command", { sessionId, command });
}

/** Initialize the SQL environment */
export function initSql(sessionId: string, dbType: string): Promise<string> {
  return call<string>("init_sql", { sessionId, dbType });
}

/** Run SQL */
export function runSql(sessionId: string, sqlText: string, dbType: string): Promise<QueryResult> {
  return call<QueryResult>("run_sql", { sessionId, sqlText, dbType });
}

/** Explain Plan */
export function explainPlan(sessionId: string, sqlText: string, dbType: string): Promise<QueryResult> {
  return call<QueryResult>("explain_plan", { sessionId, sqlText, dbType });
}

/** Lazy-load object tree children */
export function getTreeChildren(
  sessionId: string,
  nodeType: string,
  tableName: string,
  dbType: string,
): Promise<string[]> {
  return call<string[]>("get_tree_children", { sessionId, nodeType, tableName, dbType });
}

/** Get the table list. Returns structured { name, comment } pairs. */
export function getTableList(sessionId: string, dbType: string): Promise<TableInfo[]> {
  return call<TableInfo[]>("get_table_list", { sessionId, dbType });
}

/** Get column names (for autocomplete). Returns structured { name, comment }. */
export function getColumnList(sessionId: string, dbType: string): Promise<ColumnInfo[]> {
  return call<ColumnInfo[]>("get_column_list", { sessionId, dbType });
}

/** Get object source code */
export function getObjectSource(
  sessionId: string,
  objType: string,
  name: string,
  dbType: string,
): Promise<string[]> {
  return call<string[]>("get_object_source", { sessionId, objType, name, dbType });
}

/** Export to Excel */
export function exportExcel(
  columns: string[],
  rows: string[][],
): Promise<ExportInfo> {
  return call<ExportInfo>("export_excel", { columns, rows });
}

/** Export to CSV (saved to the user's Downloads folder) */
export function exportCsv(
  columns: string[],
  rows: string[][],
): Promise<ExportInfo> {
  return call<ExportInfo>("export_csv", { columns, rows });
}

/** Set update mode */
export function setUpdateMode(sessionId: string, enabled: boolean): Promise<string> {
  return call<string>("set_update_mode", { sessionId, enabled });
}

/** Connection status */
export function connectionStatus(): Promise<string | null> {
  return call<string | null>("connection_status");
}

/** Format SQL (frontend sql-formatter) */
export { format as formatSql } from "sql-formatter";

/** Tree node type helper */
export const TREE_TYPES: Record<string, TreeNodeType> = {
  SEQUENCE: "SEQUENCE",
  PACKAGE: "PACKAGE",
  "PACKAGE BODY": "PACKAGE BODY",
  TABLE: "TABLE",
  INDEXES: "INDEXES",
};
