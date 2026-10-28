//! HopsDB — Tauri backend entry point
//! SSH session management, SQL client, and Excel/CSV export.

mod excel;
pub mod sql;
pub mod ssh;

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tauri::State;
use tokio::sync::Mutex;

type SessionId = String;

struct SessionEntry {
    session: Arc<ssh::SshSession>,
    update_mode: Mutex<bool>,
    /// Serializes all SQL client commands (metadata + user SQL) on this session.
    /// HTML table output can mix if concurrent commands run; 单线程 per session prevents that.
    sql_client_lock: Mutex<()>,
}

static SESSION_SEQ: AtomicU64 = AtomicU64::new(1);

/// Global state: all active SSH sessions
pub struct AppState {
    sessions: Mutex<BTreeMap<SessionId, Arc<SessionEntry>>>,
}

#[derive(Debug, Serialize)]
struct ApiResult<T> {
    ok: bool,
    data: Option<T>,
    error: Option<String>,
}

impl<T: Serialize> ApiResult<T> {
    fn ok(data: T) -> Self {
        Self {
            ok: true,
            data: Some(data),
            error: None,
        }
    }
    #[allow(dead_code)]
    fn err(msg: impl Into<String>) -> Self {
        Self {
            ok: false,
            data: None,
            error: Some(msg.into()),
        }
    }
}

#[derive(Debug, Serialize)]
struct ConnectResult {
    session_id: String,
    user_host: String,
    message: String,
    transcript: Vec<String>,
    startup_complete: bool,
    sql_client_ready: bool,
    sql_client_db_type: Option<String>,
}

#[derive(Debug, Serialize)]
struct ExecCommandResult {
    lines: Vec<String>,
    sql_client_db_type: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct CommandStep {
    command: String,
    password: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct SshConnection {
    username: String,
    hostname: String,
    port: u16,
    password: Option<String>,
}

/// Helper to get a session by id
async fn get_session(
    state: &State<'_, AppState>,
    session_id: &str,
) -> Result<Arc<ssh::SshSession>, String> {
    let guard = state.sessions.lock().await;
    guard
        .get(session_id)
        .map(|entry| entry.session.clone())
        .ok_or_else(|| format!("Session not found: {session_id}"))
}

/// Connect to the structured first SSH hop, then run each raw command-set line.
#[tauri::command]
async fn connect(
    ssh: SshConnection,
    command_steps: Vec<CommandStep>,
    state: State<'_, AppState>,
) -> Result<ApiResult<ConnectResult>, String> {
    let steps: Vec<CommandStep> = command_steps
        .into_iter()
        .map(|step| CommandStep {
            command: step.command.trim().to_string(),
            password: step.password,
        })
        .filter(|step| !step.command.is_empty())
        .collect();
    let user_host = ssh_target(&ssh)?;
    let ssh_password = ssh.password.as_deref().unwrap_or("");

    let session = ssh::SshSession::connect_with_options(
        &user_host,
        ssh_password,
        ssh::SshConnectOptions::default(),
    )
    .await?;
    let session = Arc::new(session);
    let session_id = format!(
        "sess-{}-{}",
        SESSION_SEQ.fetch_add(1, Ordering::Relaxed),
        chrono_like_now()
    );
    let entry = Arc::new(SessionEntry {
        session: session.clone(),
        update_mode: Mutex::new(false),
        sql_client_lock: Mutex::new(()),
    });

    let mut transcript = vec![
        format!("Connected to {user_host}"),
    ];
    let mut sql_client_db_type = None;

    for (step_index, step) in steps.iter().enumerate() {
        transcript.push(format!("$ {}", step.command));
        let outcome = session
            .run_startup_command_with_password(&step.command, step.password.as_deref().unwrap_or(""))
            .await?;
        eprintln!(
            "[hopsdb] startup step session_id={} index={} completed={} waiting={}",
            session_id,
            step_index,
            outcome.completed,
            outcome.waiting_message.is_some()
        );
        sql_client_db_type = outcome.sql_client_db_type.clone();
        append_command_output(&mut transcript, &outcome.lines);
        if !outcome.completed {
            if let Some(message) = outcome.waiting_message {
                transcript.push(format!("[waiting] {message}"));
            }
            state
                .sessions
                .lock()
                .await
                .insert(session_id.clone(), entry);
            return Ok(ApiResult::ok(ConnectResult {
                session_id,
                user_host: user_host.clone(),
                message: format!(
                    "Connected to {}; startup command needs attention",
                    user_host
                ),
                transcript,
                startup_complete: false,
                sql_client_ready: false,
                sql_client_db_type: None,
            }));
        }
    }

    state
        .sessions
        .lock()
        .await
        .insert(session_id.clone(), entry);
    eprintln!(
        "[hopsdb] startup complete session_id={} sql_client_db_type={:?}",
        session_id, sql_client_db_type
    );
    Ok(ApiResult::ok(ConnectResult {
        session_id,
        user_host: user_host.clone(),
        message: format!("Connected to {user_host}"),
        transcript,
        startup_complete: true,
        sql_client_ready: sql_client_db_type.is_some(),
        sql_client_db_type,
    }))
}

fn ssh_target(ssh: &SshConnection) -> Result<String, String> {
    let username = ssh.username.trim();
    let hostname = ssh.hostname.trim();
    if username.is_empty() {
        return Err("SSH username is required".into());
    }
    if hostname.is_empty() {
        return Err("SSH hostname is required".into());
    }
    if ssh.port == 0 {
        return Err("SSH port must be between 1 and 65535".into());
    }
    Ok(format!("{username}@{hostname}:{}", ssh.port))
}

fn chrono_like_now() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

fn append_command_output(transcript: &mut Vec<String>, lines: &[String]) {
    if lines.is_empty() {
        transcript.push("(no output)".to_string());
    } else {
        transcript.extend(lines.iter().cloned());
    }
}

/// Disconnect
#[tauri::command]
async fn disconnect(
    session_id: String,
    state: State<'_, AppState>,
) -> Result<ApiResult<String>, String> {
    let entry = state.sessions.lock().await.remove(&session_id);
    if let Some(entry) = entry {
        entry.session.close(true).await;
    }
    Ok(ApiResult::ok("Disconnected".into()))
}

/// Execute a shell command
#[tauri::command]
async fn exec_command(
    session_id: String,
    command: String,
    state: State<'_, AppState>,
) -> Result<ApiResult<ExecCommandResult>, String> {
    let sck = get_session(&state, &session_id).await?;
    let outcome = sck.run_user_command(&command).await?;
    let sql_client_db_type = outcome.sql_client_db_type.clone();
    Ok(ApiResult::ok(ExecCommandResult {
        lines: outcome.lines,
        sql_client_db_type,
    }))
}

/// Initialize the SQL environment
#[tauri::command]
async fn init_sql(
    session_id: String,
    db_type: String,
    state: State<'_, AppState>,
) -> Result<ApiResult<String>, String> {
    let entry = {
        let guard = state.sessions.lock().await;
        guard
            .get(&session_id)
            .cloned()
            .ok_or_else(|| format!("Session not found: {session_id}"))?
    };
    // SQL client metadata is 单线程 per session (one queue)
    let _guard = entry.sql_client_lock.lock().await;
    eprintln!(
        "[hopsdb] init_sql start user={} host={}",
        entry.session.user, entry.session.host
    );
    sql::init_sql_commands(&entry.session, &db_type).await?;
    eprintln!(
        "[hopsdb] init_sql done user={} host={} db_type={}",
        entry.session.user, entry.session.host, db_type
    );
    Ok(ApiResult::ok("SQL environment initialized".into()))
}

/// Run SQL
#[tauri::command]
async fn run_sql(
    session_id: String,
    sql_text: String,
    db_type: String,
    state: State<'_, AppState>,
) -> Result<ApiResult<sql::QueryResult>, String> {
    let entry = {
        let guard = state.sessions.lock().await;
        guard
            .get(&session_id)
            .cloned()
            .ok_or_else(|| format!("Session not found: {session_id}"))?
    };

    // Backend guard: block mutating statements unless update mode is enabled.
    // Mirrors the frontend check so a direct invoke can't bypass it.
    let is_update = {
        // Extract the first alphabetic token (leading verb).
        let s: String = sql_text
            .chars()
            .filter(|&c| c.is_ascii_alphabetic() || c == '_' || c.is_ascii_whitespace() || c == '(')
            .collect();
        let first: String = s
            .split_whitespace()
            .next()
            .unwrap_or("")
            .trim_start_matches('(')
            .to_string();
        let verb = first.to_uppercase();
        matches!(
            verb.as_str(),
            "INSERT"
                | "UPDATE"
                | "DELETE"
                | "MERGE"
                | "REPLACE"
                | "CREATE"
                | "DROP"
                | "ALTER"
                | "TRUNCATE"
                | "RENAME"
                | "GRANT"
                | "REVOKE"
                | "PURGE"
                | "COMMENT"
        ) && !*entry.update_mode.lock().await
    };
    if is_update {
        return Err("This operation modifies data (enable Update Mode to run it)".into());
    }

    // SQL client metadata is 单线程 per session (one queue)
    let _guard = entry.sql_client_lock.lock().await;
    eprintln!(
        "[hopsdb] run_sql start db_type={} sql={}",
        db_type, sql_text
    );
    let result = sql::run_sql(&entry.session, &sql_text, &db_type).await?;
    eprintln!(
        "[hopsdb] run_sql done cols={:?} rows={} text_lines={}",
        result.columns,
        result.rows.len(),
        result.text.len()
    );
    Ok(ApiResult::ok(result))
}

/// Explain Plan
#[tauri::command]
async fn explain_plan(
    session_id: String,
    sql_text: String,
    db_type: String,
    state: State<'_, AppState>,
) -> Result<ApiResult<sql::QueryResult>, String> {
    let entry = {
        let guard = state.sessions.lock().await;
        guard
            .get(&session_id)
            .cloned()
            .ok_or_else(|| format!("Session not found: {session_id}"))?
    };
    // SQL client metadata is 单线程 per session (one queue)
    let _guard = entry.sql_client_lock.lock().await;
    let result = sql::explain_plan(&entry.session, &sql_text, &db_type).await?;
    Ok(ApiResult::ok(result))
}

/// Get object tree children
#[tauri::command]
async fn get_tree_children(
    session_id: String,
    node_type: String,
    table_name: String,
    db_type: String,
    state: State<'_, AppState>,
) -> Result<ApiResult<Vec<String>>, String> {
    let entry = {
        let guard = state.sessions.lock().await;
        guard
            .get(&session_id)
            .cloned()
            .ok_or_else(|| format!("Session not found: {session_id}"))?
    };
    // SQL client metadata is 单线程 per session (one queue)
    let _guard = entry.sql_client_lock.lock().await;
    eprintln!(
        "[hopsdb] get_tree_children start node_type={} table_name={} db_type={} user={} host={}",
        node_type, table_name, db_type, entry.session.user, entry.session.host
    );
    let list = sql::get_tree_children(&entry.session, &node_type, &table_name, &db_type).await?;
    eprintln!(
        "[hopsdb] get_tree_children done node_type={} table_name={} count={}",
        node_type,
        table_name,
        list.len()
    );
    Ok(ApiResult::ok(list))
}

/// Get the table list (init the object tree).
/// Returns structured { name, comment } pairs.
#[tauri::command]
async fn get_table_list(
    session_id: String,
    db_type: String,
    state: State<'_, AppState>,
) -> Result<ApiResult<Vec<sql::TableInfo>>, String> {
    let entry = {
        let guard = state.sessions.lock().await;
        guard
            .get(&session_id)
            .cloned()
            .ok_or_else(|| format!("Session not found: {session_id}"))?
    };
    // SQL client metadata is 单线程 per session (one queue)
    let _guard = entry.sql_client_lock.lock().await;
    eprintln!(
        "[hopsdb] get_table_list start db_type={} user={} host={}",
        db_type, entry.session.user, entry.session.host
    );
    let list = sql::get_table_list(&entry.session, &db_type).await?;
    eprintln!(
        "[hopsdb] get_table_list done db_type={} count={}",
        db_type,
        list.len()
    );
    Ok(ApiResult::ok(list))
}

/// Get column names (for autocomplete).
/// Returns structured { name, comment } pairs.
#[tauri::command]
async fn get_column_list(
    session_id: String,
    db_type: String,
    state: State<'_, AppState>,
) -> Result<ApiResult<Vec<sql::ColumnInfo>>, String> {
    let entry = {
        let guard = state.sessions.lock().await;
        guard
            .get(&session_id)
            .cloned()
            .ok_or_else(|| format!("Session not found: {session_id}"))?
    };
    // SQL client metadata is 单线程 per session (one queue)
    let _guard = entry.sql_client_lock.lock().await;
    eprintln!("[hopsdb] get_column_list start db_type={}", db_type);
    let list = sql::get_column_list(&entry.session, &db_type).await?;
    eprintln!(
        "[hopsdb] get_column_list done db_type={} count={}",
        db_type,
        list.len()
    );
    Ok(ApiResult::ok(list))
}

/// Get object source (double-click PACKAGE)
#[tauri::command]
async fn get_object_source(
    session_id: String,
    obj_type: String,
    name: String,
    db_type: String,
    state: State<'_, AppState>,
) -> Result<ApiResult<Vec<String>>, String> {
    let entry = {
        let guard = state.sessions.lock().await;
        guard
            .get(&session_id)
            .cloned()
            .ok_or_else(|| format!("Session not found: {session_id}"))?
    };
    // SQL client metadata is 单线程 per session (one queue)
    let _guard = entry.sql_client_lock.lock().await;
    let lines = sql::get_object_source(&entry.session, &obj_type, &name, &db_type).await?;
    Ok(ApiResult::ok(lines))
}

/// Export to Excel
#[tauri::command]
async fn export_excel(
    columns: Vec<String>,
    rows: Vec<Vec<String>>,
    state: State<'_, AppState>,
) -> Result<ApiResult<excel::ExportInfo>, String> {
    let _ = &state; // no session needed
    let mut data: BTreeMap<u32, Vec<String>> = BTreeMap::new();
    // Row 1: header
    let mut idx: u32 = 1;
    data.insert(idx, columns);
    for row in &rows {
        idx += 1;
        data.insert(idx, row.clone());
    }
    let info = excel::export_xlsx(&data)?;
    Ok(ApiResult::ok(info))
}

/// Export columns + rows to a timestamped CSV in the user's Downloads folder.
#[tauri::command]
async fn export_csv(
    columns: Vec<String>,
    rows: Vec<Vec<String>>,
    state: State<'_, AppState>,
) -> Result<ApiResult<excel::ExportInfo>, String> {
    let _ = &state; // no session needed
    let info = excel::export_csv(&columns, &rows)?;
    Ok(ApiResult::ok(info))
}

/// Set update mode
#[tauri::command]
async fn set_update_mode(
    session_id: String,
    enabled: bool,
    state: State<'_, AppState>,
) -> Result<ApiResult<String>, String> {
    let entry = {
        let guard = state.sessions.lock().await;
        guard
            .get(&session_id)
            .cloned()
            .ok_or_else(|| "Session not found".to_string())?
    };
    *entry.update_mode.lock().await = enabled;
    Ok(ApiResult::ok(if enabled {
        "Update mode enabled".into()
    } else {
        "Update mode disabled".into()
    }))
}

/// Get connection status
#[tauri::command]
async fn connection_status(
    state: State<'_, AppState>,
) -> Result<ApiResult<Option<String>>, String> {
    let guard = state.sessions.lock().await;
    let info = guard
        .values()
        .next()
        .map(|entry| format!("{}@{}", entry.session.user, entry.session.host));
    Ok(ApiResult::ok(info))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(AppState {
            sessions: Mutex::new(BTreeMap::new()),
        })
        .invoke_handler(tauri::generate_handler![
            connect,
            disconnect,
            exec_command,
            init_sql,
            run_sql,
            explain_plan,
            get_tree_children,
            get_table_list,
            get_column_list,
            get_object_source,
            export_excel,
            export_csv,
            set_update_mode,
            connection_status,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_structured_ssh_target_with_explicit_port() {
        let ssh = SshConnection {
            username: "oracle".into(),
            hostname: "127.0.0.1".into(),
            port: 2222,
            password: None,
        };
        assert_eq!(ssh_target(&ssh).unwrap(), "oracle@127.0.0.1:2222");
    }
}
