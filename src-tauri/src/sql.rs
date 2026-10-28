//! SQL execution logic — prompt detection, command dispatch, and HTML table parsing
//! for SQL clients that can emit HTML tables.

use crate::ssh::SshSession;
use serde::Serialize;

/// Query result: structured table
#[derive(Debug, Clone, Serialize, Default)]
pub struct QueryResult {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<String>>,
    /// Non-table text output (errors, PL/SQL output, etc.)
    pub text: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DbFlavor {
    Oracle,
    MySql,
    PostgreSql,
}

impl DbFlavor {
    fn from_db_type(db_type: &str) -> Self {
        match db_type.to_ascii_lowercase().as_str() {
            "mysql" | "mariadb" => Self::MySql,
            "postgres" | "postgresql" | "psql" => Self::PostgreSql,
            _ => Self::Oracle,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Oracle => "Oracle",
            Self::MySql => "MySQL",
            Self::PostgreSql => "PostgreSQL",
        }
    }
}

/// sqlplus init commands.
pub const ORACLE_INIT_SQL_COMMANDS: [&str; 7] = [
    "set pagesize 50000",
    "set linesize 9999",
    "set autocommit off",
    "set exitcommit off",
    "set numwidth 20",
    "alter session set NLS_DATE_FORMAT = 'YYYY/MM/DD HH24:MI:SS';",
    "set markup html on entmap off",
];

const POSTGRES_INIT_SQL_COMMANDS: [&str; 4] = [
    "\\pset format html",
    "\\pset pager off",
    "\\pset tuples_only off",
    "\\pset footer off",
];

/// Initialize the active SQL client for structured table output.
pub async fn init_sql_commands(sck: &SshSession, db_type: &str) -> Result<(), String> {
    let flavor = DbFlavor::from_db_type(db_type);
    match flavor {
        DbFlavor::Oracle => {
            for cmd in ORACLE_INIT_SQL_COMMANDS {
                sck.run_exec(cmd).await?;
            }
            Ok(())
        }
        DbFlavor::MySql => init_mysql_session(sck).await,
        DbFlavor::PostgreSql => {
            for cmd in POSTGRES_INIT_SQL_COMMANDS {
                sck.run_exec(cmd).await?;
            }
            ensure_html_output(sck, flavor).await
        }
    }
}

/// Configure a verified interactive MySQL session without touching the command
/// that launched it. MySQL HTML output can only be selected at client startup,
/// so users must explicitly include `--html` in their own command set.
async fn init_mysql_session(sck: &SshSession) -> Result<(), String> {
    eprintln!("[hopsdb] mysql init step=charset start");
    sck.run_exec("charset utf8mb4").await?;
    eprintln!("[hopsdb] mysql init step=charset done");
    ensure_html_output(sck, DbFlavor::MySql).await
}

/// Run a SQL statement and parse the HTML table output into a structured
/// result (with dialect-specific row limiting).
pub async fn run_sql(sck: &SshSession, sql: &str, db_type: &str) -> Result<QueryResult, String> {
    let sql = prepare_run_sql(sql, DbFlavor::from_db_type(db_type));
    let output = sck.run_exec(&format!("{sql};")).await?;
    parse_html_table(&output)
}

/// Execute EXPLAIN PLAN
pub async fn explain_plan(
    sck: &SshSession,
    sql: &str,
    db_type: &str,
) -> Result<QueryResult, String> {
    let sql = strip_sql_terminator(sql);
    match DbFlavor::from_db_type(db_type) {
        DbFlavor::Oracle => explain_plan_oracle(sck, &sql).await,
        DbFlavor::MySql | DbFlavor::PostgreSql => {
            let output = sck.run_exec(&format!("EXPLAIN {sql};")).await?;
            parse_html_table(&output)
        }
    }
}

async fn explain_plan_oracle(sck: &SshSession, sql: &str) -> Result<QueryResult, String> {
    sck.run_exec("set markup html off entmap off").await?;
    let list1 = sck.run_exec(&format!("EXPLAIN PLAN FOR {sql};")).await?;

    let mut error = false;
    let mut err_text = Vec::new();
    for line in &list1 {
        if line.contains("ORA-") {
            error = true;
            err_text.push(line.clone());
        }
    }

    let result = if error {
        QueryResult {
            columns: vec![],
            rows: vec![],
            text: err_text,
        }
    } else {
        let list2 = sck
            .run_exec("SELECT PLAN_TABLE_OUTPUT FROM TABLE(DBMS_XPLAN.DISPLAY());")
            .await?;
        QueryResult {
            columns: vec![],
            rows: vec![],
            text: list2
                .iter()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect(),
        }
    };
    // Restore HTML mode
    sck.run_exec("set markup html on entmap off").await?;
    Ok(result)
}

async fn ensure_html_output(sck: &SshSession, flavor: DbFlavor) -> Result<(), String> {
    if has_html_output(sck).await? {
        return Ok(());
    }

    let hint = match flavor {
        DbFlavor::MySql => "start mysql with -H or --html",
        DbFlavor::PostgreSql => {
            "start psql with -H/--html, or allow HopsDB to run \\pset format html"
        }
        DbFlavor::Oracle => "enable sqlplus markup html",
    };
    Err(format!(
        "{} client did not return HTML table output; {hint}.",
        flavor.label()
    ))
}

async fn has_html_output(sck: &SshSession) -> Result<bool, String> {
    let output = sck.run_exec("select 1 as HOPSDB_READY;").await?;
    Ok(output_has_ready_html_column(&output))
}

fn output_has_ready_html_column(output: &[String]) -> bool {
    let Ok(result) = parse_html_table(output) else {
        return false;
    };
    result
        .columns
        .iter()
        .any(|column| clean_cell(column).eq_ignore_ascii_case("HOPSDB_READY"))
}

fn prepare_run_sql(sql: &str, flavor: DbFlavor) -> String {
    let sql = strip_sql_terminator(sql);
    let sql_lower = sql.to_ascii_lowercase();
    if !sql_lower.trim_start().starts_with("select") {
        return sql;
    }

    match flavor {
        DbFlavor::Oracle => {
            if sql_lower.contains("rownum") {
                sql
            } else {
                format!("SELECT * FROM ( {sql} ) WHERE ROWNUM <= 1000")
            }
        }
        DbFlavor::MySql | DbFlavor::PostgreSql => {
            if has_limit_clause(&sql_lower) {
                sql
            } else {
                format!("{sql} LIMIT 1000")
            }
        }
    }
}

fn strip_sql_terminator(sql: &str) -> String {
    let mut sql = sql.trim().to_string();
    while sql.ends_with(';') || sql.ends_with('/') {
        sql.pop();
        sql = sql.trim_end().to_string();
    }
    sql
}

fn has_limit_clause(sql_lower: &str) -> bool {
    sql_lower
        .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .any(|token| token == "limit")
}

/// Parse sqlplus / HTML-table client output. Expected format:
/// <table border='1' width='100%'>
/// <tr><th>COL1</th><th>COL2</th></tr>
/// <tr><td>1</td><td>A</td></tr>
/// ...
/// </table>
fn parse_html_table(lines: &[String]) -> Result<QueryResult, String> {
    let mut result = QueryResult::default();
    let mut in_table = false;
    let mut header_done = false;
    let mut current_row: Vec<String> = Vec::new();
    let mut current_cell = String::new();
    let mut in_cell = false;

    for raw in lines {
        // Strip ANSI escape codes (occasionally mixed into sqlplus output)
        let mut line = raw.trim().to_string();

        // Detect table start
        if !in_table {
            if let Some(pos) = find_table_start(&line) {
                in_table = true;
                line = line[pos..].to_string();
            }
        }

        if in_table {
            // Process tags in the line
            let mut rest = line.as_str();
            loop {
                // Find the next tag
                let lt = rest.find('<');
                let Some(lt) = lt else {
                    if in_cell {
                        current_cell.push_str(rest);
                    }
                    break;
                };
                if in_cell {
                    current_cell.push_str(&rest[..lt]);
                }
                let gt = rest[lt..].find('>').map(|p| lt + p + 1);
                let Some(gt) = gt else { break };
                let tag = rest[lt + 1..gt - 1].to_lowercase();
                rest = &rest[gt..];

                // Handle tags
                if tag.starts_with("table") {
                    in_table = true;
                } else if tag.starts_with("/table") {
                    in_table = false;
                    header_done = true;
                    if in_cell {
                        current_cell.push_str("");
                        in_cell = false;
                    }
                    break;
                } else if tag.starts_with("tr") {
                    if !current_row.is_empty() {
                        if header_done {
                            result.rows.push(std::mem::take(&mut current_row));
                        } else {
                            // Header row
                            result.columns = std::mem::take(&mut current_row);
                            header_done = true;
                        }
                    }
                } else if tag.starts_with("/tr") {
                    if in_cell {
                        current_row.push(std::mem::take(&mut current_cell));
                        in_cell = false;
                    }
                    if header_done {
                        if !current_row.is_empty() {
                            result.rows.push(std::mem::take(&mut current_row));
                        }
                    } else if !current_row.is_empty() {
                        result.columns = std::mem::take(&mut current_row);
                        header_done = true;
                    }
                } else if tag.starts_with("th") {
                    in_cell = true;
                    current_cell.clear();
                } else if tag.starts_with("/th") {
                    if in_cell {
                        current_row.push(take_cell(&mut current_cell));
                        in_cell = false;
                    }
                } else if tag.starts_with("td") {
                    in_cell = true;
                    current_cell.clear();
                } else if tag.starts_with("/td") {
                    if in_cell {
                        current_row.push(take_cell(&mut current_cell));
                        in_cell = false;
                    }
                } else if tag.starts_with("br") || tag.starts_with("/br") {
                    if in_cell {
                        current_cell.push('\n');
                    }
                }
            }
            continue;
        }

        // Text outside tables (strip common HTML leftovers)
        let text = line
            .replace("<br>", "")
            .replace("<br/>", "")
            .replace("<br />", "")
            .replace("<pre>", "")
            .replace("</pre>", "")
            .replace("<p>", "")
            .replace("</p>", "");
        let text = decode_html_entities(text.trim());
        if !text.is_empty() && text != "SQL>" {
            result.text.push(text);
        }
    }

    // Finalize: unclosed tr
    if !current_row.is_empty() {
        if header_done {
            result.rows.push(current_row);
        } else {
            result.columns = current_row;
        }
    }

    Ok(result)
}

fn find_table_start(line: &str) -> Option<usize> {
    line.to_ascii_lowercase().find("<table")
}

fn take_cell(current_cell: &mut String) -> String {
    decode_html_entities(current_cell.trim())
        .replace("&nbsp;", " ")
        .trim()
        .to_string()
}

fn decode_html_entities(value: &str) -> String {
    value
        .replace("&nbsp;", " ")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

fn is_mysql_family(db_type: &str) -> bool {
    matches!(db_type, "mysql" | "mariadb")
}

/// Get object tree children for an expanded object-tree node
pub async fn get_tree_children(
    sck: &SshSession,
    node_type: &str,
    table_name: &str,
    db_type: &str,
) -> Result<Vec<String>, String> {
    let db = db_type.to_lowercase();
    let sql = match node_type {
        "TABLE" => {
            if is_mysql_family(&db) {
                format!(
                    "select column_name from information_schema.columns where table_schema = database() and table_name = '{}' order by ordinal_position;",
                    table_name.replace('\'', "''")
                )
            } else if db == "postgresql" || db == "postgres" {
                format!(
                    "select column_name from information_schema.columns where table_schema = current_schema() and table_name = lower('{}') order by ordinal_position;",
                    table_name.replace('\'', "''")
                )
            } else {
                format!(
                                    // Return "COLUMN_NAME" or "COLUMN_NAME(COMMENT)" (no spaces, so
                                    // the tree can split the comment out and copy the plain name).
                                    " select a.COLUMN_NAME || case when b.comments is null then '' else '(' || b.COMMENTS || ')' end name from USER_TAB_COLUMNS a left join USER_COL_COMMENTS b on a.table_name = b.table_name and a.COLUMN_NAME = b.COLUMN_NAME  where a.TABLE_NAME = '{}' order by a.COLUMN_ID;",
                                    table_name.replace('\'', "''")
                                )
            }
        }
        "SEQUENCE" => {
            if is_mysql_family(&db) {
                "select '' where 1 = 0;".to_string()
            } else if db == "postgresql" || db == "postgres" {
                "select sequence_name from information_schema.sequences order by sequence_name;"
                    .to_string()
            } else {
                "select sequence_name from all_sequences order by sequence_owner;".to_string()
            }
        }
        "VIEW" => {
            if is_mysql_family(&db) {
                "select table_name from information_schema.views where table_schema = database() order by table_name;".to_string()
            } else if db == "postgresql" || db == "postgres" {
                "select table_name from information_schema.views where table_schema = current_schema() order by table_name;".to_string()
            } else {
                "select view_name from user_views order by view_name;".to_string()
            }
        }
        "ROUTINE" => {
            if is_mysql_family(&db) {
                "select routine_name from information_schema.routines where routine_schema = database() order by routine_name;".to_string()
            } else if db == "postgresql" || db == "postgres" {
                "select proname from pg_proc p join pg_namespace n on n.oid = p.pronamespace where n.nspname = current_schema() order by proname;".to_string()
            } else {
                "select object_name from user_objects where object_type in ('PROCEDURE', 'FUNCTION') order by object_name;".to_string()
            }
        }
        "INDEX" => {
            if is_mysql_family(&db) {
                "select distinct concat(index_name, '|', table_name) from information_schema.statistics where table_schema = database() order by index_name;".to_string()
            } else if db == "postgresql" || db == "postgres" {
                "select indexname || '|' || tablename from pg_indexes where schemaname = current_schema() order by indexname;".to_string()
            } else {
                "select index_name || '|' || table_name from user_indexes order by index_name;"
                    .to_string()
            }
        }
        "PACKAGE" | "PACKAGE BODY" => {
            if is_mysql_family(&db) || db == "postgresql" || db == "postgres" {
                "select routine_name from information_schema.routines order by routine_name;"
                    .to_string()
            } else if node_type == "PACKAGE" {
                "select object_name from user_objects where object_type = 'PACKAGE' order by object_name;".to_string()
            } else {
                "select object_name from user_objects where object_type = 'PACKAGE BODY' order by object_name;".to_string()
            }
        }
        "INDEXES" => {
            if is_mysql_family(&db) {
                format!(
                    "select concat(index_name, '(', column_name, ')') from information_schema.statistics where table_schema = database() and table_name = '{}' order by index_name;",
                    table_name.replace('\'', "''")
                )
            } else if db == "postgresql" || db == "postgres" {
                format!(
                    "select indexname from pg_indexes where tablename = lower('{}') order by indexname;",
                    table_name.replace('\'', "''")
                )
            } else {
                // Oracle: one line per index under the owning table. Where the
                // index backs a constraint, show the constraint type (P=PK,
                // U=unique) directly; otherwise just the column list:
                //   INDEX_NAME [P]: col1,col2
                //   INDEX_NAME [U]: col1,col2
                //   INDEX_NAME: col1,col2
                format!(
                    "select idx.index_name || case when cons.constraint_type is null then '' else ' [' || cons.constraint_type || ']: ' end || listagg(ic.column_name, ',') within group (order by ic.column_position) from user_ind_columns ic join user_indexes idx on idx.index_name=ic.index_name and idx.table_name=ic.table_name left join user_constraints cons on cons.index_name=idx.index_name and cons.table_name=idx.table_name where ic.table_name='{}' group by idx.index_name, cons.constraint_type order by idx.index_name;",
                    table_name.replace('\'', "''")
                )
            }
        }
        _ => return Err(format!("Unknown node type: {node_type}")),
    };
    let output = sck.run_exec(&sql).await?;
    Ok(parse_simple_list(&output))
}

/// One table, with an optional comment (Oracle). Returned structured so
/// callers can read `name` and `comment` directly instead of re-splitting a
/// concatenated "NAME(COMMENT)" string.
#[derive(Debug, Clone, Serialize)]
pub struct TableInfo {
    pub name: String,
    /// Table comment (Oracle only; absent for dialects without comments)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
}

/// Initial table list.
/// Returns structured { name, comment } pairs. For MySQL/PostgreSQL the lookup
/// returns a plain name list (comment is None); for Oracle it returns two
/// columns (table name + comment) so callers don't have to re-split text.
pub async fn get_table_list(sck: &SshSession, db_type: &str) -> Result<Vec<TableInfo>, String> {
    let db = db_type.to_lowercase();
    let sql = if is_mysql_family(&db) {
        "select table_name from information_schema.tables where table_schema = database() and table_type = 'BASE TABLE' order by table_name;".to_string()
    } else if db == "postgresql" || db == "postgres" {
        "select table_name from information_schema.tables where table_schema = current_schema() and table_type = 'BASE TABLE' order by table_name;".to_string()
    } else {
        // Oracle: return table name and comment as two separate columns instead
        // of concatenating "NAME(COMMENT)" (the caller decides which to show).
        "select t.table_name, c.comments from user_tables t left join user_tab_comments c on t.table_name = c.table_name where t.table_name not like '%$%' order by t.table_name;".to_string()
    };
    let output = sck.run_exec(&sql).await?;
    let result = match parse_html_table(&output) {
        Ok(r) => r,
        Err(_) => return Ok(vec![]),
    };
    Ok(result
        .rows
        .into_iter()
        .map(|row| {
            let name = row.first().cloned().unwrap_or_default().trim().to_string();
            let comment = row.get(1).map(|c| clean_cell(&c)).filter(|c| !c.is_empty());
            TableInfo { name, comment }
        })
        .filter(|t| !t.name.is_empty())
        .collect())
}

/// One column, with an optional comment (Oracle). Structured so callers read
/// `name` and `comment` directly (paralleling TableInfo for tables).
#[derive(Debug, Clone, Serialize)]
pub struct ColumnInfo {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
}

/// Get column names for autocomplete.
/// Returns structured { name, comment } pairs. MySQL/PostgreSQL return plain
/// names (comment is None); Oracle joins user_col_comments for the comment.
pub async fn get_column_list(sck: &SshSession, db_type: &str) -> Result<Vec<ColumnInfo>, String> {
    let db = db_type.to_lowercase();
    let sql = if is_mysql_family(&db) {
        "select distinct column_name from information_schema.columns where table_schema = database() order by column_name;".to_string()
    } else if db == "postgresql" || db == "postgres" {
        "select distinct column_name from information_schema.columns where table_schema = current_schema() order by column_name;".to_string()
    } else {
        // Oracle: distinct column name + its comment (column comments from
        // user_col_comments; empty comment cells come back as "&nbsp;").
        "select distinct col.column_name, com.comments from user_tab_columns col left join (select column_name, max(comments) comments from user_col_comments group by column_name) com on col.column_name = com.column_name order by col.column_name;".to_string()
    };
    let output = sck.run_exec(&sql).await?;
    let result = match parse_html_table(&output) {
        Ok(r) => r,
        Err(_) => return Ok(vec![]),
    };
    Ok(result
        .rows
        .into_iter()
        .map(|row| {
            let name = row.first().cloned().unwrap_or_default().trim().to_string();
            let comment = row.get(1).map(|c| clean_cell(&c)).filter(|c| !c.is_empty());
            ColumnInfo { name, comment }
        })
        .filter(|c| !c.name.is_empty())
        .collect())
}

/// Get object source (e.g. double-click PACKAGE / PACKAGE BODY)
pub async fn get_object_source(
    sck: &SshSession,
    obj_type: &str,
    name: &str,
    db_type: &str,
) -> Result<Vec<String>, String> {
    let db = db_type.to_lowercase();
    let sql = if is_mysql_family(&db) {
        if obj_type.eq_ignore_ascii_case("VIEW") {
            format!("show create view `{}`;", name.replace('`', "``"))
        } else if obj_type.eq_ignore_ascii_case("INDEX") {
            format!(
                "select table_name, index_name, column_name, non_unique from information_schema.statistics where table_schema = database() and index_name = '{}' order by seq_in_index;",
                name.replace('\'', "''")
            )
        } else {
            format!(
                "select routine_type, routine_definition from information_schema.routines where routine_schema = database() and routine_name = '{}';",
                name.replace('\'', "''")
            )
        }
    } else if db == "postgresql" || db == "postgres" {
        if obj_type.eq_ignore_ascii_case("VIEW") {
            format!(
                "select pg_get_viewdef('{}'::regclass, true);",
                name.replace('\'', "''")
            )
        } else if obj_type.eq_ignore_ascii_case("INDEX") {
            format!(
                "select indexdef from pg_indexes where schemaname = current_schema() and indexname = '{}';",
                name.replace('\'', "''")
            )
        } else {
            format!(
                "select pg_get_functiondef(p.oid) from pg_proc p join pg_namespace n on n.oid = p.pronamespace where p.proname = '{}' order by p.proname;",
                name.replace('\'', "''")
            )
        }
    } else {
        if obj_type.eq_ignore_ascii_case("VIEW") {
            // user_views.text is a single LONG holding the whole view text (no
            // `line` column, unlike user_source). Do NOT order by line here.
            format!(
                "select text from user_views where view_name = '{}';",
                name.replace('\'', "''")
            )
        } else if obj_type.eq_ignore_ascii_case("INDEX") {
            format!(
                "select index_name || '(' || column_name || ')' from user_ind_columns where index_name = '{}' order by column_position;",
                name.replace('\'', "''")
            )
        } else if obj_type.eq_ignore_ascii_case("SEQUENCE") {
            // A sequence has attributes (next value / increment / cache), not
            // source text. Query all_sequences (not user_sequences) so nested /
            // non-current-user sequences like NTV2_SEQUENCE (owner MDSYS) are
            // also found — the tree lists sequences from all_sequences too.
            format!(
                "select sequence_owner || ' | min ' || min_value || ' | max ' || max_value || ' | incr ' || increment_by || ' | cache ' || cache_size || ' | last ' || last_number || ' | cycle ' || cycle_flag from all_sequences where sequence_name = '{}' order by sequence_owner;",
                name.replace('\'', "''")
            )
        } else {
            format!(
                "select text from user_source where name = '{}' and type = '{}' order by line;",
                name.replace('\'', "''"),
                obj_type.replace('\'', "''")
            )
        }
    };
    let output = sck.run_exec(&sql).await?;
    let result = parse_html_table(&output)?;
    if obj_type.eq_ignore_ascii_case("VIEW") || obj_type.eq_ignore_ascii_case("ROUTINE") {
        let kind = if obj_type.eq_ignore_ascii_case("VIEW") {
            "VIEW"
        } else {
            "ROUTINE"
        };
        let mut lines = vec![format!("{kind}: {name}")];
        if is_mysql_family(&db) || db == "postgresql" || db == "postgres" {
            for row in &result.rows {
                lines.push(String::new());
                lines.push("DEFINITION:".into());
                let text = row.join(" ").replace("&nbsp;", " ");
                lines.push(text);
            }
        } else if let Some(row) = result.rows.first() {
            lines.push(String::new());
            lines.push("DEFINITION:".into());
            lines.push(row.join(" ").replace("&nbsp;", " "));
        } else if !result.text.is_empty() {
            lines.push(String::new());
            lines.push("DEFINITION:".into());
            lines.extend(result.text.into_iter().map(|s| s.replace("&nbsp;", " ")));
        }
        return Ok(lines);
    }
    if obj_type.eq_ignore_ascii_case("SEQUENCE") {
        let mut lines = vec![format!("SEQUENCE: {}", name)];
        if is_mysql_family(&db) {
            lines.push(String::new());
            lines.push("DETAIL:".into());
            if let Some(row) = result.rows.first() {
                lines.push(row.join(" ").replace("&nbsp;", " "));
            }
        } else if db == "postgresql" || db == "postgres" {
            lines.push(String::new());
            lines.push("DETAIL:".into());
            if let Some(row) = result.rows.first() {
                lines.push(row.join(" ").replace("&nbsp;", " "));
            }
        } else if let Some(row) = result.rows.first() {
            lines.push(String::new());
            lines.push("DETAIL:".into());
            lines.push(row.join(" ").replace("&nbsp;", " "));
        } else if !result.text.is_empty() {
            lines.push(String::new());
            lines.push("DETAIL:".into());
            lines.extend(result.text.into_iter().map(|s| s.replace("&nbsp;", " ")));
        }
        return Ok(lines);
    }
    if obj_type.eq_ignore_ascii_case("INDEX") {
        let mut lines = Vec::new();
        lines.push(format!("INDEX: {}", name));
        if db == "postgresql" || db == "postgres" {
            if let Some(row) = result.rows.first() {
                if let Some(defn) = row.first() {
                    lines.push(String::new());
                    lines.push("DEFINITION:".into());
                    lines.push(defn.replace("&nbsp;", " "));
                }
            }
        } else if let Some(row) = result.rows.first() {
            let table_name = row.first().cloned().unwrap_or_default();
            let unique_flag = row.get(1).cloned().unwrap_or_default();
            let column_name = row.get(2).cloned().unwrap_or_default();
            let seq = row.get(3).cloned().unwrap_or_default();
            lines.push(format!("TABLE: {}", table_name));
            if !unique_flag.is_empty() {
                lines.push(format!(
                    "UNIQUE: {}",
                    if unique_flag == "0" || unique_flag.eq_ignore_ascii_case("UNIQUE") {
                        "YES"
                    } else {
                        "NO"
                    }
                ));
            }
            if !column_name.is_empty() {
                lines.push(format!(
                    "COLUMN: {}{}",
                    column_name,
                    if seq.is_empty() {
                        String::new()
                    } else {
                        format!(" (SEQ {})", seq)
                    }
                ));
            }
        }
        return Ok(lines);
    }
    // Source (PACKAGE / PACKAGE BODY / others) arrives as one HTML-table row per
    // source line (user_source rows). Rows are authoritative; result.text is just
    // sqlplus chatter (e.g. "12 rows selected."). Prefer rows, not text.
    let lines = if !result.rows.is_empty() {
        result
            .rows
            .into_iter()
            .map(|row| row.join(" ").replace("&nbsp;", "").trim_end().to_string())
            .collect()
    } else {
        result
            .text
            .into_iter()
            .map(|s| s.replace("&nbsp;", " "))
            .collect()
    };
    Ok(lines)
}

/// Clean a raw HTML table cell: treat `&nbsp;` (used by sqlplus for empty
/// cells) as a plain space, then trim. Returns an empty string for blank/empty
/// cells so callers can drop them instead of showing a stray "&nbsp;".
fn clean_cell(raw: &str) -> String {
    decode_html_entities(raw).trim().to_string()
}

/// Parse a simple single-column query result (HTML table → first cell of each row)
fn parse_simple_list(lines: &[String]) -> Vec<String> {
    let result = match parse_html_table(lines) {
        Ok(r) => r,
        Err(_) => return vec![],
    };
    let mut list: Vec<String> = Vec::new();
    for row in &result.rows {
        if let Some(first) = row.first() {
            let v = first.trim().to_string();
            if !v.is_empty() {
                list.push(v);
            }
        }
    }
    list
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn select_limit_is_dialect_specific() {
        assert_eq!(
            prepare_run_sql("select * from users", DbFlavor::Oracle),
            "SELECT * FROM ( select * from users ) WHERE ROWNUM <= 1000"
        );
        assert_eq!(
            prepare_run_sql("select * from users", DbFlavor::MySql),
            "select * from users LIMIT 1000"
        );
        assert_eq!(
            prepare_run_sql("select * from users", DbFlavor::PostgreSql),
            "select * from users LIMIT 1000"
        );
        assert_eq!(
            prepare_run_sql("select * from users limit 5", DbFlavor::MySql),
            "select * from users limit 5"
        );
    }

    #[test]
    fn parser_accepts_mysql_style_uppercase_html() {
        let lines = vec![
            "<TABLE BORDER=1><TR><TH>id</TH><TH>name</TH></TR>".to_string(),
            "<TR><TD>1</TD><TD>A&amp;B</TD></TR></TABLE>".to_string(),
        ];
        let parsed = parse_html_table(&lines).unwrap();
        assert_eq!(parsed.columns, vec!["id", "name"]);
        assert_eq!(parsed.rows, vec![vec!["1", "A&B"]]);
    }

    #[test]
    fn parser_accepts_postgresql_style_html() {
        let lines = vec![
            "<table border=\"1\">".to_string(),
            "  <tr>".to_string(),
            "    <th align=\"center\">table_name</th>".to_string(),
            "  </tr>".to_string(),
            "  <tr valign=\"top\">".to_string(),
            "    <td align=\"left\">orders</td>".to_string(),
            "  </tr>".to_string(),
            "</table>".to_string(),
            "<p>(1 row)<br /></p>".to_string(),
        ];
        let parsed = parse_html_table(&lines).unwrap();
        assert_eq!(parsed.columns, vec!["table_name"]);
        assert_eq!(parsed.rows, vec![vec!["orders"]]);
    }

}
