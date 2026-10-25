//! Run the shipped binary with isolated config/data, never the workspace's live provider config.
use friendo::{config::Config, model::Kind, store::Store};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    io,
    net::TcpListener,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    time::{Duration, Instant},
};

struct Cli {
    root: tempfile::TempDir,
    config: PathBuf,
    runs: PathBuf,
}

impl Cli {
    fn new(text: &str) -> Self {
        let root = tempfile::tempdir().unwrap();
        let config = root.path().join("fixture.toml");
        std::fs::write(&config, text).unwrap();
        let runs = root.path().join("runs");
        Self { root, config, runs }
    }
    fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_friendo"));
        command
            .current_dir(self.root.path())
            .arg("--config")
            .arg(&self.config)
            .arg("--data-dir")
            .arg(&self.runs);
        command
    }
    fn run(&self, args: &[&str]) -> Output {
        let mut command = self.command();
        command.args(args);
        bounded_output(command)
    }
}

fn bounded_output(mut command: Command) -> Output {
    // Files avoid pipe backpressure while imposing a deadline on the actual binary.
    let stdout = tempfile::NamedTempFile::new().unwrap();
    let stderr = tempfile::NamedTempFile::new().unwrap();
    let mut child = command
        .stdin(Stdio::null())
        .stdout(stdout.reopen().unwrap())
        .stderr(stderr.reopen().unwrap())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(15);
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!(
                "CLI timed out: {command:?}\n{}",
                std::fs::read_to_string(stderr.path()).unwrap()
            );
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    Output {
        status,
        stdout: std::fs::read(stdout.path()).unwrap(),
        stderr: std::fs::read(stderr.path()).unwrap(),
    }
}

fn success(output: Output) -> String {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

fn inspector(text: &str) -> Value {
    serde_json::from_str(&text[text.find('{').expect("doctor must expose JSON inspector")..])
        .unwrap()
}

fn snapshot(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    fn visit(root: &Path, dir: &Path, files: &mut BTreeMap<PathBuf, Vec<u8>>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                visit(root, &path, files);
            } else {
                files.insert(
                    path.strip_prefix(root).unwrap().into(),
                    std::fs::read(path).unwrap(),
                );
            }
        }
    }
    let mut files = BTreeMap::new();
    visit(root, root, &mut files);
    files
}

#[test]
fn doctor_reports_file_and_cli_precedence_without_contacting_a_provider() {
    let trap = TcpListener::bind("127.0.0.1:0").unwrap();
    trap.set_nonblocking(true).unwrap();
    let cli = Cli::new(&format!(
        "autonomy = 'normal'\neffects = 'full'\ncolor = true\nactive_provider = 'fixture'\n[providers.fixture]\nmodel = 'explicit-cli-fixture'\nendpoint = 'http://{}/v1'\napi_key_env = 'FRIENDO_CLI_TEST_KEY'",
        trap.local_addr().unwrap()
    ));
    let mut command = cli.command();
    command.env_remove("FRIENDO_CLI_TEST_KEY").args(["doctor"]);
    let text = success(bounded_output(command));
    assert!(text.contains("credential MISSING / streaming not contacted"));
    let fields = &inspector(&text)["fields"];
    assert_eq!(fields["autonomy"]["value"], "normal");
    assert_eq!(
        fields["autonomy"]["source"],
        cli.config.display().to_string()
    );
    let mut command = cli.command();
    command
        .env("FRIENDO_CLI_TEST_KEY", "synthetic-cli-credential")
        .args(["doctor", "--yolo", "--effects", "off", "--no-color"]);
    let text = success(bounded_output(command));
    assert!(text.contains("credential available or not required / streaming not contacted"));
    assert!(!text.contains("synthetic-cli-credential"));
    let fields = &inspector(&text)["fields"];
    for (field, value, source) in [
        ("autonomy", json!("yolo"), "CLI --yolo"),
        ("effects", json!("off"), "CLI --effects"),
        ("color", json!(false), "CLI --no-color"),
    ] {
        assert_eq!(fields[field]["value"], value);
        assert_eq!(fields[field]["source"], source);
    }
    assert!(
        !fields["execution"]["value"]["allow_shell"]
            .as_bool()
            .unwrap()
    );
    assert_eq!(trap.accept().unwrap_err().kind(), io::ErrorKind::WouldBlock);
    assert!(!cli.runs.exists(), "doctor must not construct a run");
}

#[test]
fn absent_default_file_uses_offline_defaults_and_empty_runs_are_read_only() {
    let root = tempfile::tempdir().unwrap();
    let mut command = Command::new(env!("CARGO_BIN_EXE_friendo"));
    command.current_dir(root.path()).arg("doctor");
    let text = success(bounded_output(command));
    assert!(text.contains("No providers configured"));
    assert_eq!(inspector(&text)["fields"]["autonomy"]["value"], "normal");
    assert!(!root.path().join(".friendo").exists());
    let cli = Cli::new("");
    assert!(success(cli.run(&["runs"])).is_empty());
    assert!(!cli.runs.exists());
}

#[test]
fn cli_rejects_bad_config_missing_goal_and_invalid_run_ids() {
    let cli = Cli::new("unknown = true");
    let output = cli.run(&["doctor"]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("unknown field"));
    std::fs::write(&cli.config, "").unwrap();
    for (args, expected) in [
        (vec!["run", "--headless"], "headless run requires a goal"),
        (
            vec!["replay", "../escape", "--headless"],
            "run id must be a UUID",
        ),
    ] {
        let output = cli.run(&args);
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains(expected));
    }
    assert!(!cli.runs.exists());
}

#[test]
fn demo_cli_records_a_repaired_synthetic_run_and_lists_it() {
    let cli = Cli::new("effects = 'off'");
    let summary: Value = serde_json::from_str(&success(cli.run(&["demo", "--headless"]))).unwrap();
    assert_eq!(summary["synthetic"], true);
    assert_eq!(summary["revision"], 2);
    assert_eq!(summary["autonomy"], "yolo");
    let id = summary["run_id"].as_str().unwrap();
    let listed = success(cli.run(&["runs"]));
    assert!(listed.contains(id) && listed.contains("SYNTHETIC"));
    let state: Value =
        serde_json::from_str(&success(cli.run(&["replay", id, "--headless"]))).unwrap();
    assert_eq!(state["restored"], true);
    assert_eq!(state["constraints"].as_array().unwrap().len(), 1);
}

#[test]
fn replay_preserves_every_file_and_never_reissues_uncertain_external_actions() {
    let trap = TcpListener::bind("127.0.0.1:0").unwrap();
    trap.set_nonblocking(true).unwrap();
    let mut config = Config::default();
    config.providers.insert(
        "local".into(),
        friendo::provider::ProviderConfig {
            model: "replay-fixture".into(),
            endpoint: format!("http://{}/v1", trap.local_addr().unwrap()),
            ..Default::default()
        },
    );
    config.execution.allowed_executables = vec![env!("CARGO_BIN_EXE_friendo-fixture").into()];
    let cli = Cli::new(&toml::to_string(&config).unwrap());
    let sentinel = cli.root.path().join("must-not-exist.pid");
    let mut store = Store::create(&cli.runs, vec![]).unwrap();
    store
        .append(
            "user_config",
            vec![],
            Kind::Started {
                demo: false,
                config: config.inspector(),
                source_revision: "fixture".into(),
            },
        )
        .unwrap();
    store
        .append(
            "user",
            vec![],
            Kind::Intent {
                revision: 1,
                statement: "retain uncertain action".into(),
                constraints: vec!["preserve evidence".into()],
            },
        )
        .unwrap();
    let context = json!({"model":"replay-fixture","messages":[{"role":"user","content":"exact replay context"}]});
    let (_, path) = store
        .artifact(
            "context",
            &serde_json::to_vec(&context).unwrap(),
            false,
            "none",
        )
        .unwrap();
    store
        .append(
            "controller",
            vec![],
            Kind::ModelRequested {
                id: "uncertain-model".into(),
                revision: 1,
                context: path,
            },
        )
        .unwrap();
    store.append("controller", vec![], Kind::ToolStarted {id:"uncertain-tool".into(),revision:1, invocation:json!({"tool":"exec","arguments":{"executable":env!("CARGO_BIN_EXE_friendo-fixture"),"args":["tree",sentinel,"1"]}})}).unwrap();
    let id = store.id.clone();
    let journal = store.dir.join("events.jsonl");
    drop(store);
    // An incomplete tail must remain intact through every form of review.
    use std::io::Write;
    std::fs::OpenOptions::new()
        .append(true)
        .open(&journal)
        .unwrap()
        .write_all(b"{\"partial\":")
        .unwrap();
    let before = snapshot(&cli.runs);
    for args in [
        vec!["runs"],
        vec!["replay", &id, "--headless"],
        vec!["replay", &id, "--headless", "--at", "2"],
        vec!["replay", &id, "--context"],
    ] {
        let output = cli.run(&args);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        if args.contains(&"--context") {
            assert_eq!(
                serde_json::from_slice::<Value>(&output.stdout).unwrap(),
                context
            );
        }
        if args.contains(&"--headless") {
            assert!(
                String::from_utf8_lossy(&output.stderr).contains("tail ignored, file unchanged")
            );
        }
        assert_eq!(
            snapshot(&cli.runs),
            before,
            "review changed retained evidence"
        );
        assert!(!sentinel.exists(), "review executed an uncertain command");
        assert_eq!(
            trap.accept().unwrap_err().kind(),
            io::ErrorKind::WouldBlock,
            "review contacted a provider"
        );
    }
    let error = cli.run(&[
        "replay",
        &id,
        "--snapshot",
        cli.root.path().join("invalid.txt").to_str().unwrap(),
        "--width",
        "1",
    ]);
    assert!(!error.status.success());
    assert!(String::from_utf8_lossy(&error.stderr).contains("snapshot size"));
    assert_eq!(snapshot(&cli.runs), before);
}
