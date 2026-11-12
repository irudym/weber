/*
* delete and start both take configs first, then children. If any function ever does the opposite,
* locking children and then waiting for configs while still holding it, two requests can block each
* other forever (a deadlock). So:

    * Never request configs while holding the children lock.
    * stop_the_process is fine as long as it releases the children lock (your .remove(name) on
      a temporary guard does that) before it reads the stop timeout from configs.
    * Check status too: if it reads configs and children, take configs first there as well.
*/

use std::{
    collections::{HashMap, VecDeque},
    path::Path,
    process::Stdio,
    sync::{
        Arc, Mutex as StdMutex,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tracing::{debug, error, info, warn};
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

use crate::{
    AppState, Shared,
    config::{ManagedProcConfig, remove_from_config, save_configs},
};

#[derive(Deserialize, Debug)]
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

enum StopResult {
    Stopped,
    Killed,
    Error(String),
}

impl StopResult {
    fn message(&self) -> String {
        match self {
            StopResult::Stopped => "stopped".to_string(),
            StopResult::Killed => "killed (SIGKILL)".to_string(),
            StopResult::Error(err) => format!("Error: {}", err),
        }
    }
}

fn bad(status: StatusCode, msg: impl Into<String>) -> (StatusCode, Json<ApiError>) {
    (status, Json(ApiError { error: msg.into() }))
}

fn validate(req: &AddRequest) -> Result<ManagedProcConfig, (StatusCode, String)> {
    let name = req.name.trim().to_owned();
    if name.is_empty()
        || name.len() > 32
        || !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err((
            StatusCode::BAD_REQUEST,
            "Name: use 1-32 letters, digits, - or _.".to_string(),
        ));
    }

    let program = req.program.trim().to_owned();
    if !Path::new(&program).is_absolute() {
        return Err((
            StatusCode::BAD_REQUEST,
            "Program: use the full path, for example /usr/bin/java.".to_string(),
        ));
    }
    if !Path::new(&program).is_file() {
        return Err((
            StatusCode::BAD_REQUEST,
            format!("Program: {program} doesn't exist."),
        ));
    }

    let args = shell_words::split(&req.args).map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            "Arguments: a quote is not closed.".to_string(),
        )
    })?;

    let dir = match req.dir.trim() {
        "" => None,
        d if Path::new(d).is_dir() => Some(d.to_owned()),
        d => {
            return Err((
                StatusCode::BAD_REQUEST,
                format!("Directory: {d} doesn't exist."),
            ));
        }
    };

    if !(1..=600).contains(&req.stop_timeout_secs) {
        return Err((
            StatusCode::BAD_REQUEST,
            "Stop timeout: use 1-600 seconds.".to_string(),
        ));
    }

    let cfg = ManagedProcConfig {
        name,
        program,
        args,
        dir,
        stop_timeout_secs: req.stop_timeout_secs,
    };

    Ok(cfg)
}

pub async fn add(
    State(st): State<Shared>,
    Json(req): Json<AddRequest>,
) -> Result<(StatusCode, Json<ManagedProcConfig>), (StatusCode, Json<ApiError>)> {
    info!("API call: POST /api/processes");
    info!("PAYLOAD: {:?}", req);
    // validate the request
    let cfg = match validate(&req) {
        Ok(cfg) => cfg,
        Err((code, err)) => {
            error!(err);
            return Err(bad(code, err));
        }
    };

    let mut configs = st.configs.write().await;
    if configs.iter().any(|c| c.name == cfg.name) {
        return Err(bad(
            StatusCode::CONFLICT,
            format!("A process named {} already exists.", cfg.name),
        ));
    }
    configs.push(cfg.clone());

    if let Err(e) = save_configs(&configs).await {
        configs.pop(); // keep memory and file in sync
        tracing::error!(error = %e, "could not save processes.toml");
        return Err(bad(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Could not save the process list.",
        ));
    }

    Ok((StatusCode::CREATED, Json(cfg)))
}

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
    config: ManagedProcConfig,
}

pub async fn status(State(st): State<Shared>) -> Json<Vec<Status>> {
    let mut children = st.procs.children.lock().await;
    let mut sys = st.procs.sys.lock().await;
    sys.refresh_processes(ProcessesToUpdate::All, true);

    let configs = st.configs.read().await;
    let mut out = Vec::with_capacity(configs.len());
    for p in &(*configs) {
        let name = &p.name;
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
                        .push_log(&name, "sys", format!("process exited ({why})"));
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

        let process = pid.and_then(|p| sys.process(sysinfo::Pid::from_u32(p)));
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
            cpu: process.map_or(0.0, |p| p.cpu_usage()),
            mem_bytes: process.map_or(0, |p| p.memory()),
            err_count,
            last_exit,
            config: p.clone(),
        });
    }
    Json(out)
}

async fn start_the_process(name: &str, st: &Arc<AppState>) -> Result<String, String> {
    let configs = st.configs.read().await;
    let Some(p) = (*configs).iter().find(|p| p.name == name) else {
        return Err("unknown process".into());
    };
    let mut children = st.procs.children.lock().await;
    if let Some(c) = children.get_mut(name) {
        if matches!(c.try_wait(), Ok(None)) {
            return Err("already running".into());
        }
    }

    let mut cmd = Command::new(&p.program);
    cmd.args(&p.args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    if let Some(dir) = &p.dir {
        cmd.current_dir(dir);
    }

    let spawned = cmd.spawn();

    let pid = match spawned {
        Ok(mut child) => {
            let pid = child.id();
            // Take the pipes BEFORE the child moves into the map.
            let stdout = child.stdout.take().expect("stdout is piped");
            let stderr = child.stderr.take().expect("stderr is piped");
            children.insert(name.to_string(), child);

            {
                let mut logs = st.procs.logs.lock().unwrap();
                let log = logs.entry(name.to_string()).or_default();
                log.err_count = 0; // fresh run, fresh error count
                log.last_exit = None;
            }
            st.procs
                .push_log(&name, "sys", format!("started, pid {}", pid.unwrap_or(0)));
            spawn_reader(st.clone(), name.to_string(), "out", stdout);
            spawn_reader(st.clone(), name.to_string(), "err", stderr);

            tracing::info!(process = %name, ?pid, "started process");
            pid
        }
        Err(e) => {
            tracing::error!(process = %name, error = %e, "failed to start process");
            st.procs
                .push_log(&name, "sys", format!("failed to start: {e}"));
            return Err(format!("failed to start: {e}"));
        }
    };
    Ok(format!("started, pid {pid:?}"))
}

pub async fn start(
    axum::extract::Path(name): axum::extract::Path<String>,
    State(st): State<Shared>,
) -> String {
    match start_the_process(&name, &st).await {
        Ok(msg) => msg,
        Err(err) => err,
    }
}

pub async fn stop(
    axum::extract::Path(name): axum::extract::Path<String>,
    State(st): State<Shared>,
) -> String {
    let msg = stop_the_process(&name, &st).await.message();

    tracing::info!(process = %name, "{msg}");
    st.procs.push_log(&name, "sys", msg.clone());
    msg
}

async fn stop_the_process(name: &str, st: &AppState) -> StopResult {
    let configs = st.configs.read().await;

    let child = st.procs.children.lock().await.remove(name); // lock released here
    let Some(mut child) = child else {
        return StopResult::Error("not running".into());
    };
    let Some(p) = (*configs).iter().find(|p| p.name == name) else {
        return StopResult::Error(format!(
            "unknown process, there is no process with name: {} in managed list",
            name
        ));
    };
    // Ask nicely first (SIGTERM), then force-kill after 5 s
    if let Some(pid) = child.id() {
        let _ = kill(Pid::from_raw(pid as i32), Signal::SIGTERM);
    }

    match tokio::time::timeout(Duration::from_secs(p.stop_timeout_secs), child.wait()).await {
        Ok(_) => StopResult::Stopped,
        Err(_) => {
            tracing::warn!(process = %name, "no exit after SIGTERM, sending SIGKILL");
            let _ = child.kill().await;
            StopResult::Killed
        }
    }
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

pub async fn delete(
    axum::extract::Path(name): axum::extract::Path<String>,
    State(st): State<Shared>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    info!(process= %name, "API call: DELETE /api/processes/{name}");

    //find the process
    if !st.configs.read().await.iter().any(|p| p.name == name) {
        return Err(bad(
            StatusCode::NOT_FOUND,
            format!(
                "Unknown process, there is no process with name: {} in managed list",
                name
            ),
        ));
    }

    // stop the process
    let running = st.procs.children.lock().await.contains_key(&name);
    if running {
        if let StopResult::Error(err) = stop_the_process(&name, &st).await {
            return Err(bad(StatusCode::INTERNAL_SERVER_ERROR, err));
        }
    }

    // remove the entry and update the records
    let mut configs = st.configs.write().await;

    // re-check while holding the write lock. From here on, no start can begin,
    // because start needs a read lock on configs
    {
        let mut children = st.procs.children.lock().await;
        let running_again = children
            .get_mut(&name)
            .is_some_and(|c| matches!(c.try_wait(), Ok(None))); // Ok(none) = still running
        if running_again {
            return Err(bad(
                StatusCode::CONFLICT,
                format!(
                    "{name} was started again while it was being stopped. Stop it and delete it again."
                ),
            ));
        }
        children.remove(&name);
    } // release the lock

    if !remove_from_config(&mut configs, &name) {
        return Err(bad(
            StatusCode::NOT_FOUND,
            "There's no process named {name} in the configs.",
        ));
    }

    save_configs(&configs).await.map_err(|err| {
        error!(error = %err, "could not save processes.toml");
        bad(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Could not save the process list.",
        )
    })?;
    drop(configs);

    // forget the log, so a new process with the same name starts clean
    st.procs.logs.lock().unwrap().remove(&name);

    info!(process=%name, "Process {name} deleted.");

    Ok(StatusCode::NO_CONTENT)
}

pub async fn update_with_restart(
    axum::extract::Path(name): axum::extract::Path<String>,
    State(st): State<Shared>,
    Json(req): Json<AddRequest>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    info!(process= %name, "API call: PUT /api/processes/{name}");
    debug!(?req, "payload");

    // validate fields
    let cfg = validate(&req).map_err(|(code, err)| {
        warn!(process = %name, "invalid update: {err}");
        bad(code, err)
    })?;
    if cfg.name != name {
        return Err(bad(
            StatusCode::BAD_REQUEST,
            "The name of a process can't be changed.",
        ));
    }

    // apply changes
    // find the process
    let mut configs = st.configs.write().await;
    let Some(i) = configs.iter().position(|p| p.name == name) else {
        return Err(bad(
            StatusCode::NOT_FOUND,
            format!("There's no process named {name}."),
        ));
    };

    // update the record
    let old = std::mem::replace(&mut configs[i], cfg);
    if let Err(e) = save_configs(&configs).await {
        configs[i] = old;
        error!(error = %e, "could not save processes.toml");
        return Err(bad(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Could not save the process list.",
        ));
    }
    drop(configs);

    // stop the process
    match stop_the_process(&name, &st).await {
        StopResult::Stopped | StopResult::Killed => {
            if let Err(err) = start_the_process(&name, &st).await {
                warn!("Cannot start the process: {}", name);
                // TODO: Report a failed restart as a successful save with a warning, for example 200 with { "warning": "Saved, but the restart failed: …" },
                //       not as a 500, because the change itself was stored.
                return Err(bad(StatusCode::INTERNAL_SERVER_ERROR, err));
            }
        }
        StopResult::Error(err) => {
            warn!("restarting the process: {}", err);
        }
    }

    // start the process

    Ok(StatusCode::NO_CONTENT)
}

pub async fn update(
    axum::extract::Path(name): axum::extract::Path<String>,
    State(st): State<Shared>,
    Json(req): Json<AddRequest>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    info!(process= %name, "API call: PUT /api/processes/{name}");
    debug!(?req, "payload");

    // validate fields
    let cfg = validate(&req).map_err(|(code, err)| {
        warn!(process = %name, "invalid update: {err}");
        bad(code, err)
    })?;
    if cfg.name != name {
        return Err(bad(
            StatusCode::BAD_REQUEST,
            "The name of a process can't be changed.",
        ));
    }

    // find the process
    let mut configs = st.configs.write().await;
    let Some(i) = configs.iter().position(|p| p.name == name) else {
        return Err(bad(
            StatusCode::NOT_FOUND,
            format!("There's no process named {name}."),
        ));
    };

    // update the record
    let old = std::mem::replace(&mut configs[i], cfg);
    if let Err(e) = save_configs(&configs).await {
        configs[i] = old; //rollback
        error!(error = %e, "could not save processes.toml");
        return Err(bad(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Could not save the process list.",
        ));
    }

    Ok(StatusCode::NO_CONTENT)
}
