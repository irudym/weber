use std::{
    collections::{HashMap, VecDeque},
    path::Path,
    process::Stdio,
    sync::{
        Mutex as StdMutex,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tracing::{info, warn};
extern crate shell_words;

use axum::{
    Json,
    extract::{Query, State},
    http::StatusCode,
};
use nix::{
    sys::signal::{Signal, kill},
    unistd::Pid,
};
use serde::{Deserialize, Serialize};
use sysinfo::{ProcessesToUpdate, System};
use tokio::{
    io::{AsyncBufReadExt, AsyncRead, BufReader},
    process::{Child, Command},
    sync::Mutex,
};

use crate::Shared;

#[derive(Clone, Serialize, Deserialize)]
pub struct ManagedProcConfig {
    pub name: String,
    pub program: String,
    pub args: Vec<String>,
    pub dir: Option<String>,
    pub stop_timeout_secs: u64,
}

#[derive(Deserialize)]
pub struct AddRequest {
    name: String,
    program: String,
    #[serde(default)]
    args: String,
    #[serde(default)]
    dir: String,
    stop_timeout_secs: u64,
}

#[derive(Serialize)]
pub struct ApiError {
    error: String,
}

fn bad(status: StatusCode, msg: impl Into<String>) -> (StatusCode, Json<ApiError>) {
    (status, Json(ApiError { error: msg.into() }))
}

pub async fn add(
    State(st): State<Shared>,
    Json(req): Json<AddRequest>,
) -> Result<(StatusCode, Json<ManagedProcConfig>), (StatusCode, Json<ApiError>)> {
    let name = req.name.trim().to_owned();
    if name.is_empty()
        || name.len() > 32
        || !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(bad(
            StatusCode::BAD_REQUEST,
            "Name: use 1-32 letters, digits, - or _.",
        ));
    }

    let program = req.program.trim().to_owned();
    if !Path::new(&program).is_absolute() {
        return Err(bad(
            StatusCode::BAD_REQUEST,
            "Program: use the full path, for example /usr/bin/java.",
        ));
    }
    if !Path::new(&program).is_file() {
        return Err(bad(
            StatusCode::BAD_REQUEST,
            format!("Program: {program} doesn't exist."),
        ));
    }

    let args = shell_words::split(&reg.args).map_err(|_| bad(StatusCode::BAD_REQUEST. "Arguments: a quote is not closed."))?;
    let dir = match req.dir.trim() {
        "" => None,
        d if Path::new(d).is_dir() => Some(d.to_owned()),
        d => return Err(bad(StatusCode::BAD_REQUEST, format!("Directory: {d} doesn't exist.")));
    }

    if !(1..=600!).contains(&req.stop_timeout_secs) {
        return Err(bad(StatusCode::BAD_REQUEST, "Stop timeout: use 1-600 seconds."));
    }

    let cfg = ManagedProcConfig {
        name,
        program,
        args,
        dir,
        stop_timeout_secs: req.stop_timeout_secs,
    };

    let mut configs = st.configs.write().await;
    if configs.iter().any(|c| c.name == cfg.name) {
        return Err(bad(StatusCode::CONFLICT, format!("A process names {} already exists.", cfg.name)));
    }
    configs.push(cfg.clone());



    Ok((StatusCode::CREATED, Json(cfg)))
}

pub struct ManagedProc {
    pub name: &'static str,
    pub program: &'static str,
    pub args: &'static [&'static str],
    pub dir: Option<&'static str>, // working directory, None = weber's own
    pub stop_timeout: Duration,    // wait this long after SIGTERM before SIGKILL
}

// processes
pub const MANAGED: &[ManagedProc] = &[ManagedProc {
    name: "minecraft",
    program: "java",
    args: &["-Xms1G", "-Xmx1G", "-jar", "paper.jar", "--nogui"],
    dir: Some("/opt/minecraft"),
    stop_timeout: Duration::from_secs(90),
}];

/// How many log lines are kept per process (older ones are dropped).
const LOG_CAPACITY: usize = 1000;

pub struct Procs {
    children: Mutex<HashMap<String, Child>>,
    sys: Mutex<System>,
    logs: StdMutex<HashMap<String, ProcLog>>,
    /// Global, ever-increasing line number. The browser asks for
    /// "everything after line N", so it only downloads new lines.
    next_seq: AtomicU64,
}

#[derive(Default)]
struct ProcLog {
    lines: VecDeque<LogLine>,
    err_count: u64,            // stderr lines since the last start
    last_exit: Option<String>, // e.g. "exit code 1" or "killed by signal"
}

#[derive(Clone, Serialize)]
pub struct LogLine {
    seq: u64,
    ts_ms: u64,           // unix time in milliseconds; the browser formats it
    stream: &'static str, // "out", "err" or "sys" (messages from weber itself)
    text: String,
}

impl Procs {
    pub fn new() -> Self {
        Self {
            children: Mutex::new(HashMap::new()),
            sys: Mutex::new(System::new().expect("sysinfo unavailable")),
            logs: StdMutex::new(HashMap::new()),
            next_seq: AtomicU64::new(1),
        }
    }

    fn push_log(&self, name: &str, stream: &'static str, text: String) {
        let line = LogLine {
            seq: self.next_seq.fetch_add(1, Ordering::Relaxed),
            ts_ms: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |d| d.as_millis() as u64),
            stream,
            text,
        };
        let mut logs = self.logs.lock().unwrap();
        let log = logs.entry(name.to_owned()).or_default();
        if stream == "err" {
            log.err_count += 1;
        }
        if log.lines.len() >= LOG_CAPACITY {
            log.lines.pop_front();
        }
        log.lines.push_back(line);
    }
}

// Reads a child's stdout or stderr line by line into the log buffer.
/// Ends by itself when the process closes the pipe (i.e. exits).
fn spawn_reader<R>(st: Shared, name: String, stream: &'static str, pipe: R)
where
    R: AsyncRead + Unpin + Send + 'static,
{
    tokio::spawn(async move {
        let mut lines = BufReader::new(pipe).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            tracing::trace!(process = %name, stream, "{line}");
            st.procs.push_log(&name, stream, line);
        }
    });
}

fn describe_exit(status: std::process::ExitStatus) -> String {
    use std::os::unix::process::ExitStatusExt;
    match (status.code(), status.signal()) {
        (Some(code), _) => format!("exit code {code}"),
        (None, Some(sig)) => format!("killed by signal {sig}"),
        _ => "exited".into(),
    }
}

#[derive(Serialize)]
pub struct Status {
    name: String,
    pid: Option<u32>,
    running: bool,
    cpu: f32,
    mem_bytes: u64,
    err_count: u64,
    last_exit: Option<String>,
}

pub async fn status(State(st): State<Shared>) -> Json<Vec<Status>> {
    let mut children = st.procs.children.lock().await;
    let mut sys = st.procs.sys.lock().await;
    sys.refresh_processes(ProcessesToUpdate::All, true);

    let mut out = Vec::with_capacity(MANAGED.len());
    for p in MANAGED {
        let name = p.name;
        let mut pid = None;
        if let Some(child) = children.get_mut(name) {
            match child.try_wait() {
                Ok(None) => pid = child.id(), // still running
                Ok(Some(exit)) => {
                    // Exited on its own: record why, and forget the handle,
                    // so this is reported once.
                    let why = describe_exit(exit);
                    tracing::warn!(process = %name, "process exited: {why}");
                    st.procs
                        .push_log(name, "sys", format!("process exited ({why})"));
                    st.procs
                        .logs
                        .lock()
                        .unwrap()
                        .entry(name.to_string())
                        .or_default()
                        .last_exit = Some(why);
                    children.remove(name);
                }
                Err(_) => {}
            }
        }

        let p = pid.and_then(|p| sys.process(sysinfo::Pid::from_u32(p)));
        let (err_count, last_exit) = st
            .procs
            .logs
            .lock()
            .unwrap()
            .get(name)
            .map_or((0, None), |l| (l.err_count, l.last_exit.clone()));

        out.push(Status {
            name: name.to_string(),
            pid,
            running: pid.is_some(),
            cpu: p.map_or(0.0, |p| p.cpu_usage()),
            mem_bytes: p.map_or(0, |p| p.memory()),
            err_count,
            last_exit,
        });
    }
    Json(out)
}

pub async fn start(axum::extract::Path(name): axum::extract::Path<String>, State(st): State<Shared>) -> String {
    let Some(p) = MANAGED.iter().find(|p| p.name == name) else {
        return "unknown process".into();
    };
    let mut children = st.procs.children.lock().await;
    if let Some(c) = children.get_mut(&name) {
        if matches!(c.try_wait(), Ok(None)) {
            return "already running".into();
        }
    }

    let mut cmd = Command::new(p.program);
    cmd.args(p.args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    if let Some(dir) = p.dir {
        cmd.current_dir(dir);
    }

    let spawned = cmd.spawn();

    match spawned {
        Ok(mut child) => {
            let pid = child.id();
            // Take the pipes BEFORE the child moves into the map.
            let stdout = child.stdout.take().expect("stdout is piped");
            let stderr = child.stderr.take().expect("stderr is piped");
            children.insert(name.clone(), child);

            {
                let mut logs = st.procs.logs.lock().unwrap();
                let log = logs.entry(name.clone()).or_default();
                log.err_count = 0; // fresh run, fresh error count
                log.last_exit = None;
            }
            st.procs
                .push_log(&name, "sys", format!("started, pid {}", pid.unwrap_or(0)));
            spawn_reader(st.clone(), name.clone(), "out", stdout);
            spawn_reader(st.clone(), name.clone(), "err", stderr);

            tracing::info!(process = %name, ?pid, "started process");
            format!("started, pid {pid:?}")
        }
        Err(e) => {
            tracing::error!(process = %name, error = %e, "failed to start process");
            st.procs
                .push_log(&name, "sys", format!("failed to start: {e}"));
            format!("failed to start: {e}")
        }
    }
}

pub async fn stop(axum::extract::Path(name): axum::extract::Path<String>, State(st): State<Shared>) -> String {
    let child = st.procs.children.lock().await.remove(&name); // lock released here
    let Some(mut child) = child else {
        return "not running".into();
    };
    let Some(p) = MANAGED.iter().find(|p| p.name == name) else {
        return format!(
            "unknown process, there is no process with name: {} in managed list",
            name
        )
        .into();
    };

    // Ask nicely first (SIGTERM), then force-kill after 5 s
    if let Some(pid) = child.id() {
        let _ = kill(Pid::from_raw(pid as i32), Signal::SIGTERM);
    }
    let msg = match tokio::time::timeout(p.stop_timeout, child.wait()).await {
        Ok(_) => "stopped".to_string(),
        Err(_) => {
            tracing::warn!(process = %name, "no exit after SIGTERM, sending SIGKILL");
            let _ = child.kill().await;
            "killed (SIGKILL)".to_string()
        }
    };
    tracing::info!(process = %name, "{msg}");
    st.procs.push_log(&name, "sys", msg.clone());
    msg
}

#[derive(Deserialize)]
pub struct LogsQuery {
    #[serde(default)]
    since: u64, // return only lines with seq > since
}

#[derive(Serialize)]
pub struct LogsResponse {
    lines: Vec<LogLine>,
}

/// GET /api/logs/{name}?since=N
pub async fn logs(
    axum::extract::Path(name): axum::extract::Path<String>,
    Query(q): Query<LogsQuery>,
    State(st): State<Shared>,
) -> Json<LogsResponse> {
    let logs = st.procs.logs.lock().unwrap();
    let lines = logs
        .get(&name)
        .map(|l| {
            l.lines
                .iter()
                .filter(|x| x.seq > q.since)
                .cloned()
                .collect()
        })
        .unwrap_or_default();
    Json(LogsResponse { lines })
}
