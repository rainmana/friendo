//! Native process execution and persistent pipe sessions.
//!
//! Output files contain redacted bytes. Only sanitized, bounded excerpts belong in a
//! terminal. The output cap is per stream; bytes beyond it are counted and drained.
use std::{
    collections::{BTreeMap, VecDeque},
    path::PathBuf,
    process::Stdio,
    sync::{Arc, Mutex},
    time::Duration,
};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWriteExt},
    process::{Child, ChildStdin, Command},
    sync::{mpsc, oneshot, watch},
    task::JoinHandle,
};
use tokio_util::sync::CancellationToken;

const EXCERPT_LIMIT: usize = 16 * 1024;
const WRITE_LIMIT: usize = 1024 * 1024;
const SESSION_LIMIT: usize = 64;
const CLEANUP_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Invocation {
    pub executable: String,
    pub args: Vec<String>,
    pub cwd: PathBuf,
    pub stdin: Option<String>,
    /// Explicit overrides. The controller must redact secret values in records/UI.
    pub env: BTreeMap<String, String>,
    /// Inherited names removed before applying explicit overrides.
    #[serde(default)]
    pub env_remove: Vec<String>,
    /// Values redacted before disk/excerpt retention, including across pipe reads.
    #[serde(default, skip_serializing)]
    pub redact: Vec<String>,
    /// Zero disables the deadline, including for persistent sessions.
    pub timeout_ms: u64,
    /// Maximum bytes retained on disk for each stream, including zero.
    pub output_limit_bytes: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProcessResult {
    pub exit_code: Option<i32>,
    pub termination: String,
    pub stdout_path: PathBuf,
    pub stderr_path: PathBuf,
    pub stdout_excerpt: String,
    pub stderr_excerpt: String,
    pub stdout_bytes: u64,
    pub stderr_bytes: u64,
    pub descendants: String,
    pub stdout_truncated: bool,
    pub stderr_truncated: bool,
    pub output_complete: bool,
    pub output_errors: Vec<String>,
}

#[derive(Default)]
struct Output {
    path: PathBuf,
    total: u64,
    visible: u64,
    stored: u64,
    truncated: bool,
    tail: VecDeque<u8>,
    eof: bool,
    error: Option<String>,
}

type SharedOutput = Arc<Mutex<Output>>;

fn locked(output: &SharedOutput) -> std::sync::MutexGuard<'_, Output> {
    output
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

impl Output {
    fn append(&mut self, bytes: &[u8]) {
        self.visible = self.visible.saturating_add(bytes.len() as u64);
        let discard = self
            .tail
            .len()
            .saturating_add(bytes.len())
            .saturating_sub(EXCERPT_LIMIT);
        self.tail.drain(..discard.min(self.tail.len()));
        self.tail.extend(
            bytes
                .iter()
                .copied()
                .skip(bytes.len().saturating_sub(EXCERPT_LIMIT)),
        );
    }

    fn excerpt(&self, since: u64) -> (String, u64) {
        let earliest = self.visible.saturating_sub(self.tail.len() as u64);
        let skipped = earliest.saturating_sub(since);
        let offset = since.saturating_sub(earliest).min(self.tail.len() as u64) as usize;
        let bytes: Vec<u8> = self.tail.iter().skip(offset).copied().collect();
        let mut text = sanitize(&String::from_utf8_lossy(&bytes));
        if text.len() > EXCERPT_LIMIT {
            let mut end = EXCERPT_LIMIT;
            while !text.is_char_boundary(end) {
                end -= 1;
            }
            text.truncate(end);
        }
        (text, skipped)
    }
}

struct Redactor {
    secrets: Arc<Vec<Vec<u8>>>,
    pending: Vec<u8>,
    longest: usize,
}

impl Redactor {
    fn new(secrets: Arc<Vec<Vec<u8>>>) -> Self {
        let longest = secrets.first().map_or(0, Vec::len);
        Self {
            secrets,
            pending: Vec::new(),
            longest,
        }
    }

    fn push(&mut self, bytes: &[u8], eof: bool) -> Vec<u8> {
        self.pending.extend_from_slice(bytes);
        let mut output = Vec::with_capacity(self.pending.len());
        let mut cursor = 0;
        while cursor < self.pending.len() {
            let remaining = &self.pending[cursor..];
            // Hold only a suffix that could become a secret, so ordinary short
            // interactive replies are not delayed by the longest configured key.
            if !eof
                && remaining.len() < self.longest
                && self
                    .secrets
                    .iter()
                    .any(|secret| secret.len() > remaining.len() && secret.starts_with(remaining))
            {
                break;
            }
            if let Some(secret) = self
                .secrets
                .iter()
                .find(|secret| self.pending[cursor..].starts_with(secret))
            {
                output.extend_from_slice(b"[REDACTED]");
                cursor += secret.len();
            } else {
                output.push(self.pending[cursor]);
                cursor += 1;
            }
        }
        self.pending.drain(..cursor);
        output
    }
}

async fn retain(file: &mut tokio::fs::File, output: &SharedOutput, bytes: &[u8], cap: u64) {
    let retain = {
        let mut state = locked(output);
        state.append(bytes);
        let retain = if state.error.is_some() {
            0
        } else {
            cap.saturating_sub(state.stored).min(bytes.len() as u64) as usize
        };
        if retain < bytes.len() {
            state.truncated = true;
        }
        retain
    };
    if retain > 0 {
        match file.write_all(&bytes[..retain]).await {
            Ok(()) => locked(output).stored += retain as u64,
            Err(error) => locked(output).error = Some(format!("spool: {error}")),
        }
    }
}

async fn spool<R: AsyncRead + Unpin>(
    mut reader: R,
    mut file: tokio::fs::File,
    output: SharedOutput,
    cap: u64,
    secrets: Arc<Vec<Vec<u8>>>,
) {
    let mut buffer = [0u8; 8192];
    let mut redactor = Redactor::new(secrets);
    let mut eof = false;
    loop {
        let length = match reader.read(&mut buffer).await {
            Ok(0) => {
                eof = true;
                break;
            }
            Ok(length) => length,
            Err(error) => {
                locked(&output).error = Some(format!("read: {error}"));
                break;
            }
        };
        {
            let mut state = locked(&output);
            state.total = state.total.saturating_add(length as u64);
        }
        let bytes = redactor.push(&buffer[..length], false);
        retain(&mut file, &output, &bytes, cap).await;
    }
    let bytes = redactor.push(&[], true);
    retain(&mut file, &output, &bytes, cap).await;
    if let Err(error) = file.flush().await {
        locked(&output).error = Some(format!("flush: {error}"));
    }
    locked(&output).eof = eof;
}

struct WriteRequest {
    bytes: Vec<u8>,
    response: oneshot::Sender<std::result::Result<(), String>>,
}

async fn write_stdin(
    mut stdin: ChildStdin,
    initial: Option<String>,
    persistent: bool,
    mut requests: mpsc::Receiver<WriteRequest>,
    stop: CancellationToken,
) {
    if let Some(initial) = initial {
        let result = tokio::select! { result = stdin.write_all(initial.as_bytes()) => result, _ = stop.cancelled() => return };
        if result.is_err() {
            return;
        }
    }
    if !persistent {
        return;
    }
    loop {
        let request = tokio::select! { request = requests.recv() => match request { Some(request) => request, None => return }, _ = stop.cancelled() => return };
        let result = tokio::select! { result = stdin.write_all(&request.bytes) => result.map_err(|error| error.to_string()), _ = stop.cancelled() => Err("process stdin closed during shutdown".into()) };
        let failed = result.is_err();
        let _ = request.response.send(result);
        if failed {
            return;
        }
    }
}

struct Running {
    child: Child,
    tree: ProcessTree,
    stdout: SharedOutput,
    stderr: SharedOutput,
    stdout_task: JoinHandle<()>,
    stderr_task: JoinHandle<()>,
    stdin_task: JoinHandle<()>,
    stdin_stop: CancellationToken,
    input: mpsc::Sender<WriteRequest>,
}

impl Drop for Running {
    fn drop(&mut self) {
        self.stdin_stop.cancel();
        self.stdin_task.abort();
        self.stdout_task.abort();
        self.stderr_task.abort();
        // ProcessTree and kill_on_drop also cover dropped execution futures.
    }
}

async fn launch(invocation: &Invocation, output_dir: PathBuf, persistent: bool) -> Result<Running> {
    if invocation.redact.iter().any(|secret| secret.len() > 65_536)
        || invocation.redact.iter().map(String::len).sum::<usize>() > WRITE_LIMIT
    {
        bail!("redaction values exceed bounded limits (64KiB each, 1MiB total)");
    }
    let mut secrets: Vec<Vec<u8>> = invocation
        .redact
        .iter()
        .filter(|secret| !secret.is_empty())
        .map(|secret| secret.as_bytes().to_vec())
        .collect();
    secrets.sort_by_key(|secret| std::cmp::Reverse(secret.len()));
    secrets.dedup();
    let secrets = Arc::new(secrets);
    tokio::fs::create_dir_all(&output_dir)
        .await
        .context("create process output directory")?;
    let output_dir = tokio::fs::canonicalize(output_dir).await?;
    let nonce = uuid::Uuid::new_v4();
    let stdout_path = output_dir.join(format!("{nonce}.stdout.log"));
    let stderr_path = output_dir.join(format!("{nonce}.stderr.log"));
    let mut options = tokio::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    options.mode(0o600);
    let stdout_file = options.open(&stdout_path).await?;
    let stderr_file = options.open(&stderr_path).await?;
    let mut command = Command::new(&invocation.executable);
    for name in &invocation.env_remove {
        command.env_remove(name);
    }
    command
        .args(&invocation.args)
        .current_dir(&invocation.cwd)
        .envs(&invocation.env)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(unix)]
    command.process_group(0);
    let mut child = command
        .spawn()
        .with_context(|| format!("spawn {}", invocation.executable))?;
    let tree = ProcessTree::new(&child);
    let stdout = Arc::new(Mutex::new(Output {
        path: stdout_path,
        ..Output::default()
    }));
    let stderr = Arc::new(Mutex::new(Output {
        path: stderr_path,
        ..Output::default()
    }));
    let stdout_task = tokio::spawn(spool(
        child.stdout.take().context("stdout pipe unavailable")?,
        stdout_file,
        stdout.clone(),
        invocation.output_limit_bytes,
        secrets.clone(),
    ));
    let stderr_task = tokio::spawn(spool(
        child.stderr.take().context("stderr pipe unavailable")?,
        stderr_file,
        stderr.clone(),
        invocation.output_limit_bytes,
        secrets,
    ));
    let stdin_stop = CancellationToken::new();
    let (input, requests) = mpsc::channel(8);
    let stdin_task = tokio::spawn(write_stdin(
        child.stdin.take().context("stdin pipe unavailable")?,
        invocation.stdin.clone(),
        persistent,
        requests,
        stdin_stop.clone(),
    ));
    Ok(Running {
        child,
        tree,
        stdout,
        stderr,
        stdout_task,
        stderr_task,
        stdin_task,
        stdin_stop,
        input,
    })
}

async fn finish(mut running: Running, timeout_ms: u64, cancel: CancellationToken) -> ProcessResult {
    let deadline = async {
        if timeout_ms == 0 {
            std::future::pending::<()>().await;
        }
        tokio::time::sleep(Duration::from_millis(timeout_ms)).await;
    };
    let (mut status, mut termination) = tokio::select! {
        status = running.child.wait() => (Some(status), "exited"),
        _ = cancel.cancelled() => (None, "cancelled"),
        _ = deadline => (None, "timeout"),
    };
    let mut errors = Vec::new();
    if status.is_none() {
        running.tree.force();
        if let Err(error) = running.child.start_kill() {
            errors.push(format!("child kill request: {error}"));
        }
        match tokio::time::timeout(CLEANUP_TIMEOUT, running.child.wait()).await {
            Ok(result) => status = Some(result),
            Err(_) => {
                termination = "outcome_unknown";
                errors.push("child exit was not observed after kill request".into());
            }
        }
    }
    let exit_code = match status {
        Some(Ok(status)) => status.code(),
        Some(Err(error)) => {
            termination = "outcome_unknown";
            errors.push(format!("wait: {error}"));
            None
        }
        None => None,
    };
    running.stdin_stop.cancel();
    let descendants = running.tree.cleanup().await;
    if tokio::time::timeout(CLEANUP_TIMEOUT, async {
        let (out, err) = tokio::join!(&mut running.stdout_task, &mut running.stderr_task);
        if let Err(error) = out {
            errors.push(format!("stdout reader: {error}"));
        }
        if let Err(error) = err {
            errors.push(format!("stderr reader: {error}"));
        }
    })
    .await
    .is_err()
    {
        running.stdout_task.abort();
        running.stderr_task.abort();
        errors.push("output drain incomplete: pipe remained open after cleanup deadline".into());
    }
    let stdout = locked(&running.stdout);
    let stderr = locked(&running.stderr);
    if let Some(error) = &stdout.error {
        errors.push(format!("stdout {error}"));
    }
    if let Some(error) = &stderr.error {
        errors.push(format!("stderr {error}"));
    }
    ProcessResult {
        exit_code,
        termination: termination.into(),
        stdout_path: stdout.path.clone(),
        stderr_path: stderr.path.clone(),
        stdout_excerpt: stdout.excerpt(0).0,
        stderr_excerpt: stderr.excerpt(0).0,
        stdout_bytes: stdout.total,
        stderr_bytes: stderr.total,
        descendants,
        stdout_truncated: stdout.truncated,
        stderr_truncated: stderr.truncated,
        output_complete: stdout.eof && stderr.eof && errors.is_empty(),
        output_errors: errors,
    }
}

/// Execute directly; shell syntax is interpreted only by an explicitly invoked shell.
pub async fn execute(
    invocation: Invocation,
    output_dir: PathBuf,
    cancel: CancellationToken,
) -> Result<ProcessResult> {
    let running = launch(&invocation, output_dir, false).await?;
    Ok(finish(running, invocation.timeout_ms, cancel).await)
}

struct Session {
    pid: u32,
    cancel: CancellationToken,
    input: mpsc::Sender<WriteRequest>,
    stdout: SharedOutput,
    stderr: SharedOutput,
    result: watch::Receiver<Option<ProcessResult>>,
    stdout_cursor: u64,
    stderr_cursor: u64,
}

/// Sessions keep native pipes open between calls. Interactive terminal/PTY behavior
/// and console control events on Windows are intentionally unsupported.
#[derive(Default)]
pub struct SessionManager {
    sessions: BTreeMap<String, Session>,
}

impl Drop for SessionManager {
    fn drop(&mut self) {
        for session in self.sessions.values() {
            session.cancel.cancel();
        }
    }
}

impl SessionManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn start(&mut self, invocation: Invocation, output_dir: PathBuf) -> Result<String> {
        if self.sessions.len() >= SESSION_LIMIT {
            bail!(
                "session limit ({SESSION_LIMIT}) reached; restart the application to clear completed sessions"
            );
        }
        let running = launch(&invocation, output_dir, true).await?;
        let cancel = CancellationToken::new();
        let (sender, result) = watch::channel(None);
        let id = uuid::Uuid::new_v4().to_string();
        self.sessions.insert(
            id.clone(),
            Session {
                pid: running.tree.pid,
                cancel: cancel.clone(),
                input: running.input.clone(),
                stdout: running.stdout.clone(),
                stderr: running.stderr.clone(),
                result,
                stdout_cursor: 0,
                stderr_cursor: 0,
            },
        );
        tokio::spawn(async move {
            let result = finish(running, invocation.timeout_ms, cancel).await;
            let _ = sender.send(Some(result));
        });
        Ok(id)
    }

    pub async fn read(&mut self, id: &str) -> Result<Value> {
        let session = self
            .sessions
            .get_mut(id)
            .context("unknown process session")?;
        let result = session.result.borrow().clone();
        let stdout = locked(&session.stdout);
        let stderr = locked(&session.stderr);
        let (out, stdout_skipped) = stdout.excerpt(session.stdout_cursor);
        let (err, stderr_skipped) = stderr.excerpt(session.stderr_cursor);
        session.stdout_cursor = stdout.visible;
        session.stderr_cursor = stderr.visible;
        Ok(
            json!({ "session_id": id, "pid": session.pid, "status": if result.is_some() { "completed" } else { "running" }, "transport": "native_pipes_no_pty", "stdout_excerpt": out, "stderr_excerpt": err, "stdout_path": stdout.path, "stderr_path": stderr.path, "stdout_bytes": stdout.total, "stderr_bytes": stderr.total, "stdout_stored_bytes": stdout.stored, "stderr_stored_bytes": stderr.stored, "stdout_skipped_bytes": stdout_skipped, "stderr_skipped_bytes": stderr_skipped, "stdout_truncated": stdout.truncated, "stderr_truncated": stderr.truncated, "result": result }),
        )
    }

    pub async fn write(&mut self, id: &str, input: &str) -> Result<()> {
        if input.len() > WRITE_LIMIT {
            bail!("session write exceeds {WRITE_LIMIT} byte limit");
        }
        let session = self.sessions.get(id).context("unknown process session")?;
        if session.result.borrow().is_some() {
            bail!("process session has completed");
        }
        let (response, received) = oneshot::channel();
        let request = WriteRequest {
            bytes: input.as_bytes().to_vec(),
            response,
        };
        tokio::time::timeout(Duration::from_secs(5), async {
            session
                .input
                .send(request)
                .await
                .context("process stdin is closed")?;
            received
                .await
                .context("process stdin closed before write completed")?
                .map_err(anyhow::Error::msg)
        })
        .await
        .context("session write timed out; delivery may be partial")??;
        Ok(())
    }

    pub async fn interrupt(&mut self, id: &str) -> Result<()> {
        let session = self.sessions.get(id).context("unknown process session")?;
        if session.result.borrow().is_some() {
            bail!("process session has completed");
        }
        interrupt_group(session.pid)
    }

    pub async fn wait(&mut self, id: &str, timeout_ms: u64) -> Result<Value> {
        let session = self
            .sessions
            .get_mut(id)
            .context("unknown process session")?;
        let timed_out = if session.result.borrow().is_none() {
            match tokio::time::timeout(Duration::from_millis(timeout_ms), session.result.changed())
                .await
            {
                Ok(Ok(())) => false,
                Ok(Err(_)) => bail!("session monitor ended without recording an outcome"),
                Err(_) => true,
            }
        } else {
            false
        };
        let mut value = self.read(id).await?;
        value["wait_timed_out"] = json!(timed_out);
        Ok(value)
    }

    pub async fn terminate(&mut self, id: &str) -> Result<Value> {
        self.sessions
            .get(id)
            .context("unknown process session")?
            .cancel
            .cancel();
        self.wait(id, 6_000).await
    }

    pub async fn shutdown(&mut self) -> Result<Vec<Value>> {
        for session in self.sessions.values() {
            session.cancel.cancel();
        }
        let ids: Vec<String> = self.sessions.keys().cloned().collect();
        let mut results = Vec::new();
        for id in ids {
            results.push(self.wait(&id, 6_000).await?);
        }
        Ok(results)
    }
}

struct ProcessTree {
    pid: u32,
    armed: bool,
    #[cfg(windows)]
    job: Option<usize>,
    #[cfg(windows)]
    setup_error: Option<String>,
}

impl ProcessTree {
    fn new(child: &Child) -> Self {
        let pid = child.id().unwrap_or(0);
        #[cfg(windows)]
        {
            let (job, setup_error) = windows_job(child);
            Self {
                pid,
                armed: true,
                job,
                setup_error,
            }
        }
        #[cfg(not(windows))]
        Self { pid, armed: true }
    }

    fn force(&self) {
        #[cfg(unix)]
        if self.pid > 0 {
            unsafe {
                libc::kill(-(self.pid as i32), libc::SIGKILL);
            }
        }
        #[cfg(windows)]
        if let Some(job) = self.job {
            unsafe {
                windows_sys::Win32::System::JobObjects::TerminateJobObject(job as _, 1);
            }
        }
    }

    async fn cleanup(&mut self) -> String {
        #[cfg(unix)]
        {
            let present = || self.pid > 0 && unsafe { libc::kill(-(self.pid as i32), 0) } == 0;
            if !present() {
                self.armed = false;
                return "unix_process_group: group_absent_observed; escaped_descendants_untracked"
                    .into();
            }
            self.force();
            for _ in 0..30 {
                tokio::time::sleep(Duration::from_millis(10)).await;
                if !present() {
                    self.armed = false;
                    return "unix_process_group: cleanup_requested; group_absent_observed; escaped_descendants_untracked".into();
                }
            }
            "unix_process_group: cleanup_requested; group_still_present_incomplete (may include zombies); escaped_descendants_untracked".into()
        }
        #[cfg(windows)]
        {
            use windows_sys::Win32::System::JobObjects::{
                JOBOBJECT_BASIC_ACCOUNTING_INFORMATION, JobObjectBasicAccountingInformation,
                QueryInformationJobObject,
            };
            self.force();
            if let Some(job) = self.job {
                for _ in 0..30 {
                    let mut info: JOBOBJECT_BASIC_ACCOUNTING_INFORMATION =
                        unsafe { std::mem::zeroed() };
                    let ok = unsafe {
                        QueryInformationJobObject(
                            job as _,
                            JobObjectBasicAccountingInformation,
                            &mut info as *mut _ as _,
                            std::mem::size_of_val(&info) as u32,
                            std::ptr::null_mut(),
                        )
                    };
                    if ok != 0 && info.ActiveProcesses == 0 {
                        self.armed = false;
                        return "windows_job: cleanup_requested; job_empty_observed; pre_assignment_spawn_race_unverified".into();
                    }
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
                "windows_job: cleanup_requested; active_processes_unknown_incomplete".into()
            } else {
                format!(
                    "windows_job: unavailable; descendants_unknown_incomplete: {}",
                    self.setup_error.as_deref().unwrap_or("unknown")
                )
            }
        }
        #[cfg(not(any(unix, windows)))]
        {
            "unsupported_platform: descendants_unknown_incomplete".into()
        }
    }
}

impl Drop for ProcessTree {
    fn drop(&mut self) {
        if self.armed {
            self.force();
        }
        #[cfg(windows)]
        if let Some(job) = self.job {
            unsafe {
                windows_sys::Win32::Foundation::CloseHandle(job as _);
            }
        }
    }
}

#[cfg(unix)]
fn interrupt_group(pid: u32) -> Result<()> {
    if pid == 0 {
        bail!("process id unavailable");
    }
    if unsafe { libc::kill(-(pid as i32), libc::SIGINT) } != 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(())
}

#[cfg(not(unix))]
fn interrupt_group(_pid: u32) -> Result<()> {
    bail!("interrupt unsupported for native pipe sessions on this platform; terminate is available")
}

#[cfg(windows)]
fn windows_job(child: &Child) -> (Option<usize>, Option<String>) {
    use windows_sys::Win32::{
        Foundation::CloseHandle,
        System::JobObjects::{
            AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
            JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
            SetInformationJobObject,
        },
    };
    unsafe {
        let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
        if job.is_null() {
            return (None, Some(std::io::Error::last_os_error().to_string()));
        }
        let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        let configured = SetInformationJobObject(
            job,
            JobObjectExtendedLimitInformation,
            &info as *const _ as _,
            std::mem::size_of_val(&info) as u32,
        );
        let assigned = child
            .raw_handle()
            .map(|process| AssignProcessToJobObject(job, process as _))
            .unwrap_or(0);
        if configured == 0 || assigned == 0 {
            let error = std::io::Error::last_os_error().to_string();
            CloseHandle(job);
            (None, Some(error))
        } else {
            (Some(job as usize), None)
        }
    }
}

/// Remove terminal control sequences (CSI, OSC, DCS, SOS, PM, APC, C0/C1) and
/// directional override/isolate controls. Redacted byte logs remain available.
pub fn sanitize(input: &str) -> String {
    #[derive(Clone, Copy)]
    enum State {
        Text,
        Escape,
        Csi,
        String,
        StringEscape,
    }
    let mut state = State::Text;
    let mut output = String::with_capacity(input.len());
    for ch in input.chars() {
        state = match state {
            State::Text => match ch {
                '\u{1b}' => State::Escape,
                '\u{9b}' => State::Csi,
                '\u{90}' | '\u{98}' | '\u{9d}' | '\u{9e}' | '\u{9f}' => State::String,
                '\n' | '\t' => {
                    output.push(ch);
                    State::Text
                }
                ch if ch.is_control()
                    || matches!(ch, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}') =>
                {
                    State::Text
                }
                _ => {
                    output.push(ch);
                    State::Text
                }
            },
            State::Escape => match ch {
                '[' => State::Csi,
                ']' | 'P' | 'X' | '^' | '_' => State::String,
                '\u{1b}' => State::Escape,
                ' '..='/' => State::Escape,
                _ => State::Text,
            },
            State::Csi => match ch {
                '\u{1b}' => State::Escape,
                '@'..='~' => State::Text,
                _ => State::Csi,
            },
            State::String => match ch {
                '\u{7}' | '\u{9c}' => State::Text,
                '\u{1b}' => State::StringEscape,
                _ => State::String,
            },
            State::StringEscape => match ch {
                '\\' | '\u{7}' | '\u{9c}' => State::Text,
                '\u{1b}' => State::StringEscape,
                _ => State::String,
            },
        };
    }
    output
}
