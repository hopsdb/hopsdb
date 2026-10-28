// Type definitions corresponding to the Rust backend

/** Unified return wrapper from the backend */
export interface ApiResult<T> {
  ok: boolean;
  data: T | null;
  error: string | null;
}

/** SQL query result (matches Rust QueryResult) */
export interface QueryResult {
  columns: string[];
  rows: string[][];
  /** Non-table text output (errors, PL/SQL output, etc.) */
  text: string[];
}

/** One database table, with an optional comment (Oracle). Returned structured
 * so callers read `name` / `comment` directly instead of splitting a joined
 * "NAME(COMMENT)" string. */
export interface TableInfo {
  name: string;
  comment?: string;
}

/** One column, with an optional comment (Oracle). Structured like TableInfo. */
export interface ColumnInfo {
  name: string;
  comment?: string;
}

/** Excel export result */
export interface ExportInfo {
  file_path: string;
  rows: number;
  cols: number;
}

/** One startup command with an optional password for that exact line. */
export interface CommandStep {
  command: string;
  password?: string;
}

/** First SSH hop. Commands in a command set run only after this connection. */
export interface SshConnection {
  username: string;
  hostname: string;
  port: number;
  password?: string;
}

/** Object tree node type */
export type TreeNodeType =
  | "SEQUENCE"
  | "PACKAGE"
  | "PACKAGE BODY"
  | "TABLE"
  | "INDEXES"
  | "VIEW"
  | "ROUTINE"
  | "INDEX";

/** Object tree node */
export interface TreeNode {
  id: string;
  label: string;
  type?: TreeNodeType;
  children?: TreeNode[];
  /** Owning table name for index/column nodes that query by their parent table */
  ownerTable?: string;
  /** Object comment/alias shown alongside the name (Oracle column/table comments).
   * Not included when the node name is copied. */
  comment?: string;
  /** Read-only source tab (opened by double-clicking PACKAGE) */
  source?: boolean;
}

/** Tab kind */
export type TabKind = "shell" | "sql" | "source";

export interface TabItem {
  id: string;
  kind: TabKind;
  title: string;
  /** Editor content for sql/source tabs */
  content?: string;
  /** Object type for source tabs (PACKAGE / PACKAGE BODY) */
  objType?: string;
}
