//! SSH session management
//! Core logic: connect → keepalive heartbeat → runExec (write command, detect
//! command end via prompt regex, auto-answer password prompts)
//!
//! The read half runs as a dedicated background task that continuously drains
//! the channel into a shared buffer; run_exec reads from that buffer. This
//! avoids russh's one-shot `wait()` missing late data (which caused silent
//! shell commands like `export` to hang for the full timeout).

use russh::client::Msg;
use russh::keys::{load_secret_key, PrivateKeyWithHashAlg, PublicKeyOrCertificate};
use russh::{ChannelMsg, ChannelWriteHalf, Pty};
use serde::Serialize;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;

/// Custom handler: accept any server key (equivalent to StrictHostKeyChecking=no)
#[derive(Clone)]
pub struct ClientHandler;

impl russh::client::Handler for ClientHandler {
    type Error = russh::Error;

    fn check_server_key(
        &mut self,
        _key: &PublicKeyOrCertificate,
    ) -> impl std::future::Future<Output = Result<bool, Self::Error>> + Send {
        async { Ok(true) }
    }
}

/// Prompt regex used to detect the end of a command's output
pub const LINUX_PROMPT_REGEX: [&str; 14] = [
    "~]#",
    "~#",
    "#",
    ":~#",
    "/$",
    "SQL>",
    "~]$",
    "]#",
    "SQL&gt;",
    "$",
    "password:",
    "connecting (yes/no)?",
    "Password:",
    "パスワード:",
];

const SHELL_CLOSED_MESSAGE: &str = "Session closed. Reconnect to run more commands.";

/// Check whether a line ends with a prompt
pub fn ends_with_prompt(line: &str) -> bool {
    let trimmed = line.trim_end();
    if is_postgresql_continuation_prompt(trimmed) {
        return false;
    }
    LINUX_PROMPT_REGEX.iter().any(|p| trimmed.ends_with(p))
        || is_mysql_prompt(trimmed)
        || is_postgresql_prompt(trimmed)
}

/// Output of a single command execution
#[derive(Debug, Clone, Serialize)]
pub struct ExecResult {
    /// All output lines
    pub lines: Vec<String>,
}

#[allow(dead_code)]
impl ExecResult {
    pub fn new(lines: Vec<String>) -> Self {
        Self { lines }
    }
}

#[derive(Debug, Clone)]
pub struct ExecOutcome {
    pub lines: Vec<String>,
    pub completed: bool,
    /// True only when the command ended at a database client's prompt, not a
    /// regular shell prompt after a failed client launch.
    pub at_sql_client_prompt: bool,
    /// Database family inferred from the recognized default client prompt.
    pub sql_client_db_type: Option<String>,
    pub waiting_message: Option<String>,
}

/// Byte buffer shared between the reader task and run_exec.
struct Inbound {
    buf: Vec<u8>,
}

#[derive(Clone, Copy)]
enum AutoAnswerMode<'a> {
    Enabled { password: &'a str },
    Disabled,
}

impl<'a> AutoAnswerMode<'a> {
    fn password(self) -> Option<&'a str> {
        match self {
            Self::Enabled { password } if !password.is_empty() => Some(password),
            _ => None,
        }
    }

    fn auto_confirm(self) -> bool {
        matches!(self, Self::Enabled { .. })
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum IncompleteCommandBehavior {
    InterruptAndError,
    ReturnWaiting,
}

#[derive(Default)]
pub struct SshConnectOptions {
    pub identity_files: Vec<PathBuf>,
}

/// SSH session. A background reader task drains the channel into `inbound`;
/// run_exec consumes it.
pub struct SshSession {
    pub host: String,
    pub user: String,
    inbound: Arc<Mutex<Inbound>>,
    write_half: Arc<Mutex<ChannelWriteHalf<Msg>>>,
    heartbeat_wait: Arc<std::sync::atomic::AtomicU32>,
    closed: Arc<std::sync::atomic::AtomicBool>,
    /// Serializes command execution: since the reader task drains the whole
    /// channel into one shared buffer, only one run_exec may read at a time.
    /// Without this, concurrent commands (e.g. two init_sql + a get_table_list
    /// from the UI) steal each other's output.
    exec_lock: Arc<tokio::sync::Mutex<()>>,
}

impl SshSession {
    /// Connect + authenticate + open shell channel
    pub async fn connect(user_host: &str, password: &str) -> Result<Self, String> {
        Self::connect_with_options(user_host, password, SshConnectOptions::default()).await
    }

    pub async fn connect_with_options(
        user_host: &str,
        password: &str,
        options: SshConnectOptions,
    ) -> Result<Self, String> {
        let (user, host, port) = split_user_host(user_host)?;

        let config = russh::client::Config::default();
        let mut handle =
            russh::client::connect(Arc::new(config), (host.as_str(), port), ClientHandler)
                .await
                .map_err(|e| format!("SSH connect failed: {e}"))?;

        if !authenticate_key_then_password(&mut handle, &user, password, &options.identity_files)
            .await?
        {
            let message = if password.is_empty() {
                "SSH authentication failed: no usable public key succeeded and no password was provided"
            } else {
                "SSH authentication failed: no usable public key succeeded and password authentication failed"
            };
            return Err(message.into());
        }

        let channel = handle
            .channel_open_session()
            .await
            .map_err(|e| format!("Failed to open channel: {e}"))?;
        let _ = channel
            .request_pty(true, "xterm-256color", 120, 40, 0, 0, &[(Pty::ECHO, 0)])
            .await;
        channel
            .request_shell(true)
            .await
            .map_err(|e| format!("Failed to request shell: {e}"))?;

        let (read_half, write_half) = channel.split();
        let write_half = Arc::new(Mutex::new(write_half));
        let inbound = Arc::new(Mutex::new(Inbound { buf: Vec::new() }));

        let session = Self {
            host,
            user,
            inbound,
            write_half: write_half.clone(),
            heartbeat_wait: Arc::new(std::sync::atomic::AtomicU32::new(0)),
            closed: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            exec_lock: Arc::new(tokio::sync::Mutex::new(())),
        };

        // Background reader task: continuously drain the read half into the buffer.
        let inbound_read = session.inbound.clone();
        let closed = session.closed.clone();
        tokio::spawn(async move {
            let mut read_half = read_half;
            loop {
                if closed.load(std::sync::atomic::Ordering::SeqCst) {
                    break;
                }
                let maybe = read_half.wait().await;
                match maybe {
                    Some(ChannelMsg::Data { data }) => {
                        let mut g = inbound_read.lock().await;
                        g.buf.extend_from_slice(&data);
                    }
                    Some(ChannelMsg::ExtendedData { data, .. }) => {
                        let mut g = inbound_read.lock().await;
                        g.buf.extend_from_slice(b"[stderr] ");
                        g.buf.extend_from_slice(&data);
                    }
                    Some(ChannelMsg::Close | ChannelMsg::Eof) | None => {
                        // Channel ended.
                        closed.store(true, std::sync::atomic::Ordering::SeqCst);
                        break;
                    }
                    Some(_) => {}
                }
            }
        });

        // Give the initial login banner a brief chance to arrive, then discard
        // it. It must never be mistaken for the completion of the first user
        // startup command. Do not require a particular prompt format here.
        tokio::time::sleep(Duration::from_millis(750)).await;
        session.inbound.lock().await.buf.clear();
        session.spawn_heartbeat();
        Ok(session)
    }

    /// Heartbeat thread: count every 60 seconds; after 5 idle ticks (5 minutes)
    /// send an empty command to keep the session alive
    fn spawn_heartbeat(&self) {
        let write_half = self.write_half.clone();
        let counter = self.heartbeat_wait.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(60)).await;
                let n = counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
                if n >= 5 {
                    counter.store(0, std::sync::atomic::Ordering::SeqCst);
                    let w = write_half.lock().await;
                    let keepalive: bytes::Bytes = b"\n".to_vec().into();
                    let _ = w.data_bytes(keepalive).await;
                }
            }
        });
    }

    /// Write one command line to the channel
    async fn write_command(&self, cmd: &str) -> Result<(), String> {
        if self.closed.load(std::sync::atomic::Ordering::SeqCst) {
            return Err("SSH shell is closed; reconnect to run more commands".into());
        }
        let result = self.write_command_unchecked(cmd).await;
        if result.is_err() {
            self.closed.store(true, std::sync::atomic::Ordering::SeqCst);
        }
        result
    }

    async fn write_command_unchecked(&self, cmd: &str) -> Result<(), String> {
        let w = self.write_half.lock().await;
        let bytes: bytes::Bytes = format!("{cmd}\n").into_bytes().into();
        w.data_bytes(bytes)
            .await
            .map_err(|e| format!("Failed to write command: {e}"))
    }

    async fn interrupt_current_command(&self) {
        let w = self.write_half.lock().await;
        let bytes: bytes::Bytes = b"\x03".to_vec().into();
        let _ = w.data_bytes(bytes).await;
        drop(w);
        tokio::time::sleep(Duration::from_millis(120)).await;
        self.inbound.lock().await.buf.clear();
    }

    /// Execute a command and wait for output until a prompt matches or timeout.
    /// Password prompts fail unless the caller uses `run_exec_with_password`.
    pub async fn run_exec(&self, command: &str) -> Result<Vec<String>, String> {
        self.run_exec_with_password(command, "").await
    }

    pub async fn run_exec_with_password(
        &self,
        command: &str,
        password: &str,
    ) -> Result<Vec<String>, String> {
        let outcome = self
            .run_exec_inner(
                command,
                AutoAnswerMode::Enabled { password },
                IncompleteCommandBehavior::InterruptAndError,
            )
            .await?;
        Ok(outcome.lines)
    }

    pub async fn run_startup_command_with_password(
        &self,
        command: &str,
        password: &str,
    ) -> Result<ExecOutcome, String> {
        self.run_exec_inner(
            command,
            AutoAnswerMode::Enabled { password },
            IncompleteCommandBehavior::ReturnWaiting,
        )
        .await
    }

    /// Execute a user-entered shell command without sending stored credentials
    /// back into the shell. Startup command sets use the separate
    /// run_startup_command_with_password path.
    pub async fn run_user_command(&self, command: &str) -> Result<ExecOutcome, String> {
        self.run_exec_inner(
            command,
            AutoAnswerMode::Disabled,
            IncompleteCommandBehavior::InterruptAndError,
        )
        .await
    }

    async fn run_exec_inner(
        &self,
        command: &str,
        auto_answer_mode: AutoAnswerMode<'_>,
        incomplete_behavior: IncompleteCommandBehavior,
    ) -> Result<ExecOutcome, String> {
        // Serialize with other commands on this session so concurrent callers
        // (e.g. two init_sql + a get_table_list) don't steal each other's output
        // from the shared reader buffer.
        let _guard = self.exec_lock.lock().await;
        if self.closed.load(std::sync::atomic::Ordering::SeqCst) {
            return Err("SSH shell is closed; reconnect to run more commands".into());
        }

        let mut current = command.to_string();
        let waits_for_prompt =
            command_waits_for_prompt(command) || auto_answer_mode.password().is_some();
        let max_polls = if waits_for_prompt { 240 } else { 80 };
        let idle_limit = if waits_for_prompt { 80 } else { 10 };
        for _round in 0..5 {
            // Drain anything left over before this command so we only look at
            // this command's output.
            {
                let mut g = self.inbound.lock().await;
                g.buf.clear();
            }
            self.write_command(&current).await?;

            let mut list: Vec<String> = Vec::new();
            let mut line = String::new();
            let mut idle_secs = 0u32;
            let mut done = false;
            let mut saw_prompt = false;
            let mut saw_sql_client_prompt = false;
            let mut sql_client_db_type = None;
            let mut saw_password_prompt = false;
            let mut saw_host_key_prompt = false;

            // Loop until we see a prompt or the buffer goes idle.
            for _ in 0..max_polls {
                let mut g = self.inbound.lock().await;
                // Pull complete lines out of the buffer.
                if !g.buf.is_empty() {
                    idle_secs = 0;
                }
                while let Some(pos) = g.buf.iter().position(|&b| b == b'\n') {
                    let raw: Vec<u8> = g.buf.drain(..=pos).collect();
                    let s = String::from_utf8_lossy(&raw);
                    let s = sanitize_terminal_text(s.trim_end_matches(['\r', '\n']));
                    line = s.clone();
                    if should_collect_output_line(&s) && !is_echoed_input_line(&s, &current) {
                        // Only cap an absurd number of lines (prevent runaway
                        // memory), and drop the OLDEST lines rather than wiping
                        // the whole buffer, so large result sets stay intact.
                        if list.len() >= 30_000 {
                            list.remove(0);
                        }
                        list.push(s);
                    }
                }
                // Check the remaining (no-newline) tail for a prompt.
                let cur = sanitize_terminal_text(&String::from_utf8_lossy(&g.buf));
                if is_password_prompt(&line) || is_password_prompt(&cur) {
                    saw_password_prompt = true;
                    done = true;
                } else if is_host_key_confirmation_prompt(&line)
                    || is_host_key_confirmation_prompt(&cur)
                {
                    saw_host_key_prompt = true;
                    done = true;
                } else if let Some(db_type) = detect_sql_client_db_type(&line)
                    .or_else(|| detect_sql_client_db_type(&cur))
                {
                    saw_prompt = true;
                    saw_sql_client_prompt = true;
                    sql_client_db_type = Some(db_type.to_string());
                    done = true;
                } else if ends_with_prompt(&line) || ends_with_prompt(&cur) {
                    saw_prompt = true;
                    done = true;
                }
                drop(g);

                if done {
                    break;
                }
                if self.closed.load(std::sync::atomic::Ordering::SeqCst) {
                    break;
                }

                // No progress this round; poll the buffer again (short sleep).
                tokio::time::sleep(Duration::from_millis(50)).await;
                idle_secs += 1;
                // A command that produces no prompt (e.g. silent `export`) has
                // essentially no output; a short idle window is enough to call it
                // done. Interactive transitions such as ssh/sqlplus wait longer.
                if idle_secs >= idle_limit {
                    break;
                }
            }

            // Flush any remaining tail.
            {
                let mut g = self.inbound.lock().await;
                if !g.buf.is_empty() {
                    let s = sanitize_terminal_text(&String::from_utf8_lossy(&g.buf));
                    line = s.clone();
                    if is_password_prompt(&s) {
                        saw_password_prompt = true;
                    } else if is_host_key_confirmation_prompt(&s) {
                        saw_host_key_prompt = true;
                    } else if let Some(db_type) = detect_sql_client_db_type(&s) {
                        saw_prompt = true;
                        saw_sql_client_prompt = true;
                        sql_client_db_type = Some(db_type.to_string());
                    } else if ends_with_prompt(&s) {
                        saw_prompt = true;
                    }
                    if should_collect_output_line(&s) && !is_echoed_input_line(&s, &current) {
                        list.push(s.trim().to_string());
                    }
                    g.buf.clear();
                }
            }

            if self.closed.load(std::sync::atomic::Ordering::SeqCst) {
                list.push(SHELL_CLOSED_MESSAGE.to_string());
                return Ok(ExecOutcome {
                    lines: list,
                    completed: true,
                    at_sql_client_prompt: false,
                    sql_client_db_type: None,
                    waiting_message: None,
                });
            }

            // Auto-answer recursively
            if saw_password_prompt || is_password_prompt(&line) {
                if let Some(password) = auto_answer_mode.password() {
                    current = password.to_string();
                    continue;
                }
                if incomplete_behavior == IncompleteCommandBehavior::ReturnWaiting {
                    collect_waiting_prompt(&mut list, &line);
                    return Ok(ExecOutcome {
                        lines: list,
                        completed: false,
                        at_sql_client_prompt: false,
                        sql_client_db_type: None,
                        waiting_message: Some(
                            "Command is waiting for a password. Configure Pwd for this line, or continue manually here."
                                .into(),
                        ),
                    });
                }
                self.interrupt_current_command().await;
                if auto_answer_mode.auto_confirm() {
                    return Err(
                        "Command requires a password, but no password is configured for this line"
                            .into(),
                    );
                }
                return Err(
                    "Command is waiting for a password. Put this line in the connection command set and enable Pwd, or use SSH key authentication."
                        .into(),
                );
            }
            if saw_host_key_prompt || is_host_key_confirmation_prompt(&line) {
                if auto_answer_mode.auto_confirm() {
                    current = "yes".to_string();
                    continue;
                }
                if incomplete_behavior == IncompleteCommandBehavior::ReturnWaiting {
                    collect_waiting_prompt(&mut list, &line);
                    return Ok(ExecOutcome {
                        lines: list,
                        completed: false,
                        at_sql_client_prompt: false,
                        sql_client_db_type: None,
                        waiting_message: Some(
                            "Command is waiting for SSH host-key confirmation. Continue manually here, or keep this line in the startup command set."
                                .into(),
                        ),
                    });
                }
                self.interrupt_current_command().await;
                return Err(
                    "Command is waiting for SSH host-key confirmation. Put this line in the connection command set so it can be confirmed safely."
                        .into(),
                );
            }
            if waits_for_prompt && !saw_prompt {
                if incomplete_behavior == IncompleteCommandBehavior::ReturnWaiting {
                    return Ok(ExecOutcome {
                        lines: list,
                        completed: false,
                        at_sql_client_prompt: false,
                        sql_client_db_type: None,
                        waiting_message: Some(
                            "Command did not return to a shell prompt. It is still active; continue from Command view."
                                .into(),
                        ),
                    });
                }
                self.interrupt_current_command().await;
                return Err("Command did not return to a shell prompt; it was interrupted to keep the session usable.".into());
            }
            if !auto_answer_mode.auto_confirm() && is_sql_client_prompt(&line) {
                collect_waiting_prompt(&mut list, &line);
            }

            return Ok(ExecOutcome {
                lines: list,
                completed: true,
                at_sql_client_prompt: saw_sql_client_prompt,
                sql_client_db_type,
                waiting_message: None,
            });
        }
        Err("Auto-answer exceeded 5 rounds, aborting".into())
    }

    /// Close the connection. With `exit=true` it sends several `exit` commands
    /// to unwrap the sqlplus/shell session down to the SSH level, then writes
    /// a final exit. These are fire-and-forget writes (no prompt wait), so
    /// disconnect returns immediately instead of stalling ~1.5s waiting for
    /// the sqlplus prompt that never comes back after exit.
    pub async fn close(&self, exit: bool) {
        if exit {
            let _ = self.write_command_unchecked("exit").await;
            let _ = self.write_command_unchecked("exit").await;
            let _ = self.write_command_unchecked("exit").await;
        }
        let _ = self.write_command_unchecked("exit").await;
        self.closed.store(true, std::sync::atomic::Ordering::SeqCst);
    }
}

async fn authenticate_key_then_password(
    handle: &mut russh::client::Handle<ClientHandler>,
    user: &str,
    password: &str,
    identity_files: &[PathBuf],
) -> Result<bool, String> {
    if try_key_files(handle, user, password, identity_files).await? {
        return Ok(true);
    }

    if try_default_key_files(handle, user, password).await? {
        return Ok(true);
    }

    if try_agent_auth(handle, user).await? {
        return Ok(true);
    }

    if password.is_empty() {
        return Ok(false);
    }

    let auth = handle
        .authenticate_password(user.to_string(), password)
        .await
        .map_err(|e| format!("SSH password auth failed: {e}"))?;
    Ok(auth.success())
}

#[cfg(unix)]
async fn try_agent_auth(
    handle: &mut russh::client::Handle<ClientHandler>,
    user: &str,
) -> Result<bool, String> {
    let mut agent = match russh::keys::agent::client::AgentClient::connect_env().await {
        Ok(agent) => agent,
        Err(_) => return Ok(false),
    };

    let identities = match agent.request_identities().await {
        Ok(identities) => identities,
        Err(_) => return Ok(false),
    };

    for identity in identities {
        let rsa_hash = best_supported_rsa_hash(handle).await?;
        let auth = match identity {
            russh::keys::agent::AgentIdentity::PublicKey { key, .. } => {
                handle
                    .authenticate_publickey_with(user.to_string(), key, rsa_hash, &mut agent)
                    .await
            }
            russh::keys::agent::AgentIdentity::Certificate { certificate, .. } => {
                handle
                    .authenticate_certificate_with(
                        user.to_string(),
                        certificate,
                        rsa_hash,
                        &mut agent,
                    )
                    .await
            }
        };

        if matches!(auth, Ok(result) if result.success()) {
            return Ok(true);
        }
    }

    Ok(false)
}

#[cfg(not(unix))]
async fn try_agent_auth(
    _handle: &mut russh::client::Handle<ClientHandler>,
    _user: &str,
) -> Result<bool, String> {
    Ok(false)
}

async fn try_default_key_files(
    handle: &mut russh::client::Handle<ClientHandler>,
    user: &str,
    password: &str,
) -> Result<bool, String> {
    try_key_files(handle, user, password, &default_private_key_paths()).await
}

async fn try_key_files(
    handle: &mut russh::client::Handle<ClientHandler>,
    user: &str,
    password: &str,
    paths: &[PathBuf],
) -> Result<bool, String> {
    for path in paths {
        let key = match load_default_key(&path, password) {
            Ok(key) => key,
            Err(_) => continue,
        };
        let rsa_hash = best_supported_rsa_hash(handle).await?;
        let auth = handle
            .authenticate_publickey(
                user.to_string(),
                PrivateKeyWithHashAlg::new(Arc::new(key), rsa_hash),
            )
            .await;

        if matches!(auth, Ok(result) if result.success()) {
            return Ok(true);
        }
    }

    Ok(false)
}

async fn best_supported_rsa_hash(
    handle: &russh::client::Handle<ClientHandler>,
) -> Result<Option<russh::keys::HashAlg>, String> {
    handle
        .best_supported_rsa_hash()
        .await
        .map(|hash| hash.flatten())
        .map_err(|e| format!("SSH auth negotiation failed: {e}"))
}

fn default_private_key_paths() -> Vec<PathBuf> {
    let Some(home) = dirs::home_dir() else {
        return Vec::new();
    };

    ["id_ed25519", "id_ecdsa", "id_rsa", "id_dsa"]
        .iter()
        .map(|file| home.join(".ssh").join(file))
        .filter(|path| path.is_file())
        .collect()
}

fn load_default_key(
    path: &std::path::Path,
    password: &str,
) -> Result<russh::keys::PrivateKey, String> {
    match load_secret_key(path, None) {
        Ok(key) => Ok(key),
        Err(first_error) => {
            if password.is_empty() {
                return Err(first_error.to_string());
            }
            load_secret_key(path, Some(password)).map_err(|second_error| second_error.to_string())
        }
    }
}

fn command_waits_for_prompt(command: &str) -> bool {
    let parts = shlex::split(command).unwrap_or_else(|| {
        // An unfinished quote is still an interactive command until the shell
        // returns a real prompt. Fall back to whitespace splitting so startup
        // does not falsely report success after a Bash continuation prompt.
        command.split_whitespace().map(str::to_owned).collect()
    });
    let Some(program) = parts.into_iter().find_map(|part| {
        if is_shell_assignment(&part) {
            return None;
        }
        Some(
            part.rsplit('/')
                .next()
                .unwrap_or(&part)
                .to_ascii_lowercase(),
        )
    }) else {
        return false;
    };
    matches!(
        program.as_str(),
        "ssh" | "slogin" | "sqlplus" | "sudo" | "su" | "mysql" | "psql"
    )
}

fn is_shell_assignment(arg: &str) -> bool {
    let Some((name, _)) = arg.split_once('=') else {
        return false;
    };
    let mut chars = name.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn should_collect_output_line(line: &str) -> bool {
    let trimmed = line.trim();
    !trimmed.is_empty()
        && !ends_with_prompt(trimmed)
        && !is_password_prompt(trimmed)
        && !is_host_key_confirmation_prompt(trimmed)
        && !trimmed.contains("[sudo] password for")
}

fn is_echoed_input_line(line: &str, command: &str) -> bool {
    let line = line.trim();
    let command = command.trim();
    if command.is_empty() {
        return false;
    }
    line == command || (line.ends_with(command) && line.len().saturating_sub(command.len()) <= 2)
}

fn sanitize_terminal_text(text: &str) -> String {
    let stripped = strip_terminal_sequences(text);
    let visible = stripped.rsplit('\r').next().unwrap_or("").trim_end();
    visible
        .chars()
        .filter(|c| *c == '\t' || !c.is_control())
        .collect()
}

fn strip_terminal_sequences(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();

    while let Some(ch) = chars.next() {
        match ch {
            '\u{1b}' => match chars.next() {
                Some('[') => skip_csi_sequence(&mut chars),
                Some(']') => skip_osc_sequence(&mut chars),
                Some(_) | None => {}
            },
            '\u{9b}' => skip_csi_sequence(&mut chars),
            _ => out.push(ch),
        }
    }

    out
}

fn skip_csi_sequence<I>(chars: &mut std::iter::Peekable<I>)
where
    I: Iterator<Item = char>,
{
    for ch in chars.by_ref() {
        if ('@'..='~').contains(&ch) {
            break;
        }
    }
}

fn skip_osc_sequence<I>(chars: &mut std::iter::Peekable<I>)
where
    I: Iterator<Item = char>,
{
    let mut saw_escape = false;
    for ch in chars.by_ref() {
        if ch == '\u{7}' {
            break;
        }
        if saw_escape && ch == '\\' {
            break;
        }
        saw_escape = ch == '\u{1b}';
    }
}

fn collect_waiting_prompt(lines: &mut Vec<String>, line: &str) {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return;
    }
    if !matches!(lines.last(), Some(last) if last == trimmed) {
        lines.push(trimmed.to_string());
    }
}

fn is_password_prompt(text: &str) -> bool {
    let trimmed = text.trim_end();
    let lower = trimmed.to_ascii_lowercase();
    lower.ends_with("password:")
        || trimmed.ends_with("パスワード:")
        || (lower.contains("[sudo] password for") && trimmed.ends_with(':'))
}

fn is_host_key_confirmation_prompt(text: &str) -> bool {
    let lower = text.trim_end().to_ascii_lowercase();
    lower.ends_with("continue connecting (yes/no)?")
        || lower.ends_with("continue connecting (yes/no/[fingerprint])?")
}

pub fn detect_sql_client_db_type(text: &str) -> Option<&'static str> {
    let prompt = last_visible_prompt_fragment(text);
    let lower = prompt.to_ascii_lowercase();
    if prompt == "SQL>" || prompt == "SQL&gt;" {
        Some("Oracle")
    } else if lower == "mariadb>" || (lower.contains("mariadb [") && lower.ends_with("]>")) {
        Some("MariaDB")
    } else if is_mysql_prompt(prompt) {
        Some("MySQL")
    } else if is_postgresql_prompt(prompt) {
        Some("PostgreSQL")
    } else {
        None
    }
}

fn is_sql_client_prompt(text: &str) -> bool {
    detect_sql_client_db_type(text).is_some()
}

fn is_mysql_prompt(text: &str) -> bool {
    let trimmed = last_visible_prompt_fragment(text);
    let lower = trimmed.to_ascii_lowercase();
    lower.ends_with("mysql>")
        || ((lower.contains("mysql [") || lower.contains("mariadb [")) && lower.ends_with("]>"))
}

fn is_postgresql_prompt(text: &str) -> bool {
    let trimmed = last_visible_prompt_fragment(text);
    let Some(db_name) = trimmed
        .strip_suffix("=>")
        .or_else(|| trimmed.strip_suffix("=#"))
    else {
        return false;
    };
    !db_name.is_empty()
        && db_name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
}

fn is_postgresql_continuation_prompt(text: &str) -> bool {
    let trimmed = last_visible_prompt_fragment(text);
    let Some(prompt_state) = trimmed.chars().rev().nth(1) else {
        return false;
    };
    let Some(privilege) = trimmed.chars().next_back() else {
        return false;
    };
    if !matches!(prompt_state, '-' | '*' | '\'' | '"' | '$' | '(')
        || !matches!(privilege, '>' | '#')
    {
        return false;
    }
    let db_name = &trimmed[..trimmed.len() - 2];
    !db_name.is_empty()
        && db_name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
}

fn last_visible_prompt_fragment(text: &str) -> &str {
    text.rsplit(['\n', '\r'])
        .find(|part| !part.trim().is_empty())
        .unwrap_or(text)
        .trim()
}

impl Drop for SshSession {
    fn drop(&mut self) {
        self.closed.store(true, std::sync::atomic::Ordering::SeqCst);
    }
}

/// Split "user@host[:port]" into parts (port defaults to 22)
pub fn split_user_host(user_host: &str) -> Result<(String, String, u16), String> {
    let (user, rest) = user_host
        .split_once('@')
        .ok_or_else(|| "Format should be user@host[:port]".to_string())?;
    if user.is_empty() || rest.is_empty() {
        return Err("Format should be user@host[:port]".into());
    }
    let (host, port) = match rest.split_once(':') {
        Some((h, p)) => {
            let port: u16 = p.parse().map_err(|_| format!("Invalid port: {p}"))?;
            (h.to_string(), port)
        }
        None => (rest.to_string(), 22),
    };
    if host.is_empty() {
        return Err("Format should be user@host[:port]".into());
    }
    Ok((user.to_string(), host, port))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn password_prompt_detection_requires_prompt_suffix() {
        assert!(is_password_prompt("oracle@127.0.0.1's password:"));
        assert!(is_password_prompt("[sudo] password for oracle:"));
        assert!(is_password_prompt("パスワード:"));

        assert!(!is_password_prompt(
            "Permission denied (publickey,password)."
        ));
        assert!(!is_password_prompt("passwords.txt"));
        assert!(!is_password_prompt("last password update succeeded"));
    }

    #[test]
    fn host_key_confirmation_detection_requires_prompt_suffix() {
        assert!(is_host_key_confirmation_prompt(
            "Are you sure you want to continue connecting (yes/no)?"
        ));
        assert!(is_host_key_confirmation_prompt(
            "Are you sure you want to continue connecting (yes/no/[fingerprint])?"
        ));

        assert!(!is_host_key_confirmation_prompt(
            "Host key verification failed."
        ));
    }

    #[test]
    fn terminal_control_sequences_are_removed_from_output() {
        assert_eq!(
            sanitize_terminal_text("\u{1b}[?2004l/home/ubuntu\u{1b}[0m"),
            "/home/ubuntu"
        );
        assert_eq!(
            sanitize_terminal_text("\u{1b}[0m\u{1b}[01;34mgitRepos\u{1b}[0m"),
            "gitRepos"
        );
        assert_eq!(sanitize_terminal_text("\u{1b}]0;title\u{7}pwd"), "pwd");
    }

    #[test]
    fn echoed_input_lines_are_detected() {
        assert!(is_echoed_input_line("pwd", "pwd"));
        assert!(is_echoed_input_line("eexit", "exit"));
        assert!(!is_echoed_input_line("/home/ubuntu", "pwd"));
    }

    #[test]
    fn mysql_and_postgresql_prompts_are_detected() {
        assert!(ends_with_prompt("mysql>"));
        assert!(ends_with_prompt("MariaDB [shop]>"));
        assert!(ends_with_prompt("MySQL [shop]>"));
        assert!(ends_with_prompt("BBMySQL [dealradar]>"));
        assert!(ends_with_prompt(
            "Reading table information for completion of table and column names\nBBMySQL [dealradar]>"
        ));
        assert!(ends_with_prompt("postgres=>"));
        assert!(ends_with_prompt("app_db=#"));
        assert!(!ends_with_prompt("postgres-#"));
        assert!(!ends_with_prompt("row value=>"));
    }

    #[test]
    fn database_type_is_inferred_from_default_client_prompts() {
        assert_eq!(detect_sql_client_db_type("mysql>"), Some("MySQL"));
        assert_eq!(
            detect_sql_client_db_type("MariaDB [ec-cube]>"),
            Some("MariaDB")
        );
        assert_eq!(detect_sql_client_db_type("mariadb>"), Some("MariaDB"));
        assert_eq!(detect_sql_client_db_type("SQL>"), Some("Oracle"));
        assert_eq!(detect_sql_client_db_type("app_db=>"), Some("PostgreSQL"));
        assert_eq!(detect_sql_client_db_type("/home/user $"), None);
    }

    #[test]
    fn execution_outcome_keeps_the_detected_database_type_when_prompt_output_is_hidden() {
        let outcome = ExecOutcome {
            lines: Vec::new(),
            completed: true,
            at_sql_client_prompt: true,
            sql_client_db_type: Some("MySQL".to_string()),
            waiting_message: None,
        };
        assert_eq!(outcome.sql_client_db_type.as_deref(), Some("MySQL"));
    }

    #[test]
    fn interactive_program_detection_allows_env_prefix_and_paths() {
        assert!(command_waits_for_prompt(
            "MYSQL_PWD=secret mysql --html -u app shop"
        ));
        assert!(command_waits_for_prompt(
            "/usr/bin/psql --html -U app -d shop"
        ));
        assert!(command_waits_for_prompt("sqlplus system/password@db"));
        assert!(!command_waits_for_prompt("MYSQL_PWD=secret echo mysql"));
    }

    #[test]
    fn malformed_mysql_startup_command_still_waits_for_a_prompt() {
        assert!(command_waits_for_prompt(
            "mysql -uroot -p'unterminated --default-character-set=utf8mb4 -D release_success"
        ));
    }

}
