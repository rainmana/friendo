use friendo::process::{Invocation, SessionManager, execute, sanitize};
use std::{collections::BTreeMap, path::PathBuf, time::Duration};
use tokio_util::sync::CancellationToken;

fn invocation(args: &[&str]) -> Invocation {
    Invocation {
        executable: env!("CARGO_BIN_EXE_friendo-fixture").into(),
        args: args.iter().map(|arg| (*arg).into()).collect(),
        cwd: std::env::current_dir().unwrap(),
        stdin: None,
        env: BTreeMap::new(),
        env_remove: vec![],
        redact: vec![],
        timeout_ms: 5_000,
        output_limit_bytes: 32_768,
    }
}

#[tokio::test]
async fn direct_arguments_are_literal_without_a_shell() {
    let dir = tempfile::tempdir().unwrap();
    let result = execute(
        invocation(&[
            "args",
            "spaces stay together",
            "$(not-a-shell)",
            "a; exit 99",
        ]),
        dir.path().into(),
        CancellationToken::new(),
    )
    .await
    .unwrap();
    assert_eq!(result.exit_code, Some(0));
    assert!(
        result
            .stdout_excerpt
            .contains("spaces stay together\n$(not-a-shell)\na; exit 99")
    );
    assert_eq!(result.termination, "exited");
    assert!(result.output_complete, "{:?}", result.output_errors);
}

#[tokio::test]
async fn large_stdout_stderr_and_stdin_are_drained_concurrently_with_disk_caps() {
    let dir = tempfile::tempdir().unwrap();
    let mut call = invocation(&["stdin", "1048576", "1048576"]);
    call.stdin = Some("x".repeat(2 * 1024 * 1024));
    let result = execute(call, dir.path().into(), CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(result.exit_code, Some(0), "{result:?}");
    assert!(result.stdout_bytes > 1_048_576);
    assert_eq!(result.stderr_bytes, 1_048_576);
    assert!(result.stdout_excerpt.contains("stdin_bytes=2097152"));
    assert!(result.stdout_excerpt.len() <= 16_384);
    assert!(result.stderr_excerpt.len() <= 16_384);
    assert!(result.stdout_truncated && result.stderr_truncated);
    assert_eq!(std::fs::metadata(result.stdout_path).unwrap().len(), 32_768);
    assert_eq!(std::fs::metadata(result.stderr_path).unwrap().len(), 32_768);
    assert!(result.output_complete, "{:?}", result.output_errors);
}

#[tokio::test]
async fn zero_disk_cap_still_drains_and_counts_output() {
    let dir = tempfile::tempdir().unwrap();
    let mut call = invocation(&["emit", "65536", "65536"]);
    call.output_limit_bytes = 0;
    let result = execute(call, dir.path().into(), CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(result.stdout_bytes, 65_536);
    assert_eq!(std::fs::metadata(result.stdout_path).unwrap().len(), 0);
    assert!(result.stdout_truncated);
    assert!(result.output_complete);
}

#[tokio::test]
async fn timeout_stops_a_native_process() {
    let dir = tempfile::tempdir().unwrap();
    let mut call = invocation(&["sleep", "60000"]);
    call.timeout_ms = 40;
    let result = execute(call, dir.path().into(), CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(result.termination, "timeout");
    assert_eq!(result.exit_code, None);
    assert_cleanup_observed(&result.descendants);
}

async fn wait_for_file(path: &std::path::Path) -> String {
    for _ in 0..500 {
        if let Ok(text) = tokio::fs::read_to_string(path).await
            && !text.is_empty()
        {
            return text;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("fixture child pid file was not created");
}

#[cfg(unix)]
async fn assert_process_gone(pid: i32) {
    for _ in 0..500 {
        if unsafe { libc::kill(pid, 0) } == -1 {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("descendant process {pid} remains visible");
}

#[cfg(windows)]
async fn assert_process_gone(pid: i32) {
    use windows_sys::Win32::{
        Foundation::{CloseHandle, ERROR_INVALID_PARAMETER, GetLastError},
        System::Threading::{GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION},
    };
    for _ in 0..500 {
        let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid as u32) };
        if handle.is_null() {
            assert_eq!(
                unsafe { GetLastError() },
                ERROR_INVALID_PARAMETER,
                "process query failed for a reason other than absence"
            );
            return;
        }
        let mut code = 0;
        let observed = unsafe { GetExitCodeProcess(handle, &mut code) };
        unsafe {
            CloseHandle(handle);
        }
        assert_ne!(observed, 0, "could not observe descendant exit state");
        if code != 259 {
            return;
        } // STILL_ACTIVE, not an observed exit.
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("descendant process {pid} remains active");
}

fn assert_cleanup_observed(observation: &str) {
    #[cfg(unix)]
    assert!(
        observation.contains("group_absent_observed"),
        "{observation}"
    );
    #[cfg(windows)]
    {
        assert!(
            observation.contains("windows_job") && observation.contains("job_empty_observed"),
            "{observation}"
        );
        assert!(
            observation.contains("pre_assignment_spawn_race_unverified"),
            "the supervision limit must remain explicit: {observation}"
        );
    }
}

#[tokio::test]
async fn cancellation_requests_and_observes_descendant_cleanup() {
    let dir = tempfile::tempdir().unwrap();
    let pid_file = dir.path().join("child.pid");
    let mut call = invocation(&["tree", pid_file.to_str().unwrap(), "60000"]);
    call.timeout_ms = 0;
    let cancel = CancellationToken::new();
    let token = cancel.clone();
    let path = dir.path().to_path_buf();
    let task = tokio::spawn(async move { execute(call, path, token).await.unwrap() });
    let pid = wait_for_file(&pid_file).await.parse::<i32>().unwrap();
    cancel.cancel();
    let result = task.await.unwrap();
    assert_eq!(result.termination, "cancelled");
    assert_cleanup_observed(&result.descendants);
    assert_process_gone(pid).await;
}

async fn read_until(manager: &mut SessionManager, id: &str, needle: &str) -> serde_json::Value {
    let mut output = String::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    while tokio::time::Instant::now() < deadline {
        let value = manager.read(id).await.unwrap();
        output.push_str(value["stdout_excerpt"].as_str().unwrap());
        if output.contains(needle) {
            return value;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("session output did not contain {needle:?}: {output:?}");
}

#[tokio::test]
async fn session_survives_separate_turns_and_reads_only_new_output() {
    let dir = tempfile::tempdir().unwrap();
    let cwd = tempfile::tempdir().unwrap();
    let mut call = invocation(&["session"]);
    call.cwd = cwd.path().into();
    call.env
        .insert("FRIENDO_FIXTURE_VALUE".into(), "explicit-value".into());
    call.timeout_ms = 0;
    let mut manager = SessionManager::new();
    let id = manager.start(call, dir.path().into()).await.unwrap();
    read_until(&mut manager, &id, "ready pid=").await;
    manager.write(&id, "echo first turn\n").await.unwrap();
    let first = read_until(&mut manager, &id, "first turn").await;
    assert_eq!(first["status"], "running");
    assert_eq!(manager.read(&id).await.unwrap()["stdout_excerpt"], "");
    tokio::time::sleep(Duration::from_millis(30)).await;
    manager.write(&id, "echo second turn\n").await.unwrap();
    read_until(&mut manager, &id, "second turn").await;
    manager
        .write(&id, "env FRIENDO_FIXTURE_VALUE\n")
        .await
        .unwrap();
    read_until(&mut manager, &id, "explicit-value").await;
    manager.write(&id, "pwd\n").await.unwrap();
    read_until(
        &mut manager,
        &id,
        cwd.path().canonicalize().unwrap().to_str().unwrap(),
    )
    .await;
    manager.write(&id, "exit 7\n").await.unwrap();
    let done = manager.wait(&id, 2_000).await.unwrap();
    assert_eq!(done["result"]["exit_code"], 7);
    assert_eq!(done["status"], "completed");
    assert!(!done["wait_timed_out"].as_bool().unwrap());
    assert!(PathBuf::from(done["stdout_path"].as_str().unwrap()).exists());
}

#[tokio::test]
async fn session_deadline_runs_while_idle_and_wait_timeout_is_non_destructive() {
    let dir = tempfile::tempdir().unwrap();
    let mut manager = SessionManager::new();
    let mut call = invocation(&["session"]);
    call.timeout_ms = 250;
    let id = manager.start(call, dir.path().into()).await.unwrap();
    let waiting = manager.wait(&id, 1).await.unwrap();
    assert_eq!(waiting["status"], "running");
    assert_eq!(waiting["wait_timed_out"], true);
    let done = manager.wait(&id, 5_000).await.unwrap();
    assert_eq!(done["status"], "completed");
    assert_eq!(done["wait_timed_out"], false);
    assert_eq!(done["result"]["termination"], "timeout");
}

#[cfg(windows)]
#[tokio::test]
async fn windows_pipe_interrupt_is_explicitly_unsupported_but_termination_is_observed() {
    let dir = tempfile::tempdir().unwrap();
    let mut manager = SessionManager::new();
    let mut call = invocation(&["session"]);
    call.timeout_ms = 0;
    let id = manager.start(call, dir.path().into()).await.unwrap();
    read_until(&mut manager, &id, "ready pid=").await;
    let error = manager.interrupt(&id).await.unwrap_err();
    assert!(error.to_string().contains("interrupt unsupported"));
    assert_eq!(manager.read(&id).await.unwrap()["status"], "running");
    let done = manager.terminate(&id).await.unwrap();
    assert_eq!(done["status"], "completed");
    assert_eq!(done["result"]["termination"], "cancelled");
    assert_cleanup_observed(done["result"]["descendants"].as_str().unwrap());
}

#[tokio::test]
async fn terminate_and_shutdown_observe_session_outcomes() {
    let dir = tempfile::tempdir().unwrap();
    let mut manager = SessionManager::new();
    let id = manager
        .start(invocation(&["session"]), dir.path().into())
        .await
        .unwrap();
    let result = manager.terminate(&id).await.unwrap();
    assert_eq!(result["result"]["termination"], "cancelled");
    let id = manager
        .start(invocation(&["session"]), dir.path().into())
        .await
        .unwrap();
    assert!(
        manager
            .write(&id, &"x".repeat(1024 * 1024 + 1))
            .await
            .is_err()
    );
    let results = manager.shutdown().await.unwrap();
    assert_eq!(results.len(), 2);
    assert!(results.iter().all(|value| value["status"] == "completed"));
}

#[test]
fn sanitization_strips_terminal_controls_without_erasing_readable_unicode() {
    let source = "hello\x1b[31mred\x1b[0m\x1b]52;c;clipboard\x07world\x1bPprivate\x1b\\!\u{9b}2J\u{9d}title\u{9c}\u{202e} café 😀\r\x08\n\t";
    assert_eq!(sanitize(source), "helloredworld! café 😀\n\t");
    assert_eq!(sanitize("before\x1b]unterminated payload"), "before");
    assert_eq!(sanitize("a\x1b(0b"), "ab");
}

#[tokio::test]
async fn executable_output_is_sanitized_for_terminal_excerpts() {
    let dir = tempfile::tempdir().unwrap();
    let result = execute(
        invocation(&["escapes"]),
        dir.path().into(),
        CancellationToken::new(),
    )
    .await
    .unwrap();
    assert_eq!(result.stdout_excerpt, "beforeredafterend\n");
    let raw = tokio::fs::read(result.stdout_path).await.unwrap();
    assert!(raw.contains(&0x1b));
}

#[tokio::test]
async fn secrets_are_redacted_across_pipe_chunks_before_disk_and_excerpt_retention() {
    let dir = tempfile::tempdir().unwrap();
    let secret = "fixture-sensitive-credential-café-123456";
    let mut call = invocation(&["secret"]);
    call.env
        .insert("FRIENDO_FIXTURE_SECRET".into(), secret.into());
    call.redact.push(secret.into());
    let result = execute(call, dir.path().into(), CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(result.stdout_excerpt, "before:[REDACTED]:after\n");
    assert_eq!(result.stderr_excerpt, "stderr:[REDACTED]\n");
    assert!(!result.stdout_truncated);
    assert!(!result.stderr_truncated);
    for path in [result.stdout_path, result.stderr_path] {
        let bytes = tokio::fs::read(path).await.unwrap();
        let text = String::from_utf8(bytes).unwrap();
        assert!(!text.contains(secret));
        assert!(text.contains("[REDACTED]"));
    }
}

#[tokio::test]
async fn redaction_does_not_delay_short_session_responses() {
    let dir = tempfile::tempdir().unwrap();
    let mut call = invocation(&["session"]);
    call.redact
        .push("secret-much-longer-than-short-session-output-12345678901234567890".into());
    let mut manager = SessionManager::new();
    let id = manager.start(call, dir.path().into()).await.unwrap();
    read_until(&mut manager, &id, "ready pid=").await;
    manager.write(&id, "echo ok\n").await.unwrap();
    read_until(&mut manager, &id, "ok").await;
    manager.terminate(&id).await.unwrap();
}

#[tokio::test]
async fn descendant_pipes_are_closed_when_the_parent_exits_first() {
    let dir = tempfile::tempdir().unwrap();
    let pid_file = dir.path().join("orphan.pid");
    let result = execute(
        invocation(&["orphan", pid_file.to_str().unwrap(), "60000"]),
        dir.path().into(),
        CancellationToken::new(),
    )
    .await
    .unwrap();
    assert_eq!(result.exit_code, Some(0));
    assert_eq!(result.termination, "exited");
    assert_process_gone(wait_for_file(&pid_file).await.parse().unwrap()).await;
    assert_cleanup_observed(&result.descendants);
    assert!(result.output_complete, "{result:?}");
}

#[tokio::test]
async fn dropping_an_execution_future_cleans_up_its_descendants() {
    let dir = tempfile::tempdir().unwrap();
    let pid_file = dir.path().join("drop.pid");
    let mut call = invocation(&["tree", pid_file.to_str().unwrap(), "60000"]);
    call.timeout_ms = 0;
    let output = dir.path().to_path_buf();
    let task = tokio::spawn(async move { execute(call, output, CancellationToken::new()).await });
    let pid = wait_for_file(&pid_file).await.parse().unwrap();
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    assert_process_gone(pid).await;
}

#[cfg(unix)]
#[tokio::test]
async fn interrupt_sends_sigint_to_the_session_group() {
    let dir = tempfile::tempdir().unwrap();
    let mut manager = SessionManager::new();
    let id = manager
        .start(invocation(&["session"]), dir.path().into())
        .await
        .unwrap();
    read_until(&mut manager, &id, "ready pid=").await;
    manager.interrupt(&id).await.unwrap();
    let result = manager.wait(&id, 2_000).await.unwrap();
    assert_eq!(result["status"], "completed");
    assert_eq!(result["result"]["termination"], "exited");
    assert_eq!(result["result"]["exit_code"], serde_json::Value::Null);
}

#[tokio::test]
async fn inherited_environment_can_be_removed_without_mutating_the_parent() {
    let dir = tempfile::tempdir().unwrap();
    // PATH is safe to inspect, inherited by this test process, and unnecessary
    // for launching the fixture because its executable path is absolute.
    let original = std::env::var("PATH").expect("test runner must provide PATH");
    assert!(!original.is_empty());
    let mut call = invocation(&["secret", "PATH"]);
    call.env_remove.push("PATH".into());
    let result = execute(call, dir.path().into(), CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(result.stdout_excerpt, "before::after\n");
    assert_eq!(result.stderr_excerpt, "stderr:\n");
    assert_eq!(std::env::var("PATH").unwrap(), original);
}

#[tokio::test]
async fn explicit_environment_overrides_are_applied_after_removals() {
    let dir = tempfile::tempdir().unwrap();
    let mut call = invocation(&["secret", "PATH"]);
    call.env_remove.push("PATH".into());
    call.env
        .insert("PATH".into(), "safe-explicit-fixture-value".into());
    let result = execute(call, dir.path().into(), CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(
        result.stdout_excerpt,
        "before:safe-explicit-fixture-value:after\n"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn process_output_files_are_private_on_unix() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let result = execute(
        invocation(&["emit", "1", "1"]),
        dir.path().into(),
        CancellationToken::new(),
    )
    .await
    .unwrap();
    for path in [result.stdout_path, result.stderr_path] {
        assert_eq!(
            std::fs::metadata(path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
}
