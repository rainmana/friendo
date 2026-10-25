use friendo::config::Config;
use std::path::Path;

fn load(root: &Path, text: &str) -> anyhow::Result<Config> {
    let path = root.join("test.toml");
    std::fs::write(&path, text)?;
    Config::load(Some(&path))
}

#[test]
fn partial_configuration_preserves_standard_execution_defaults() {
    let root = tempfile::tempdir().unwrap();
    let config = load(root.path(), "effects = 'off'\n[execution]\ntimeout_ms = 17").unwrap();
    assert_eq!(config.autonomy, "normal");
    assert_eq!(config.effects, "off");
    assert_eq!(config.execution.timeout_ms, 17);
    assert!(!config.execution.allow_shell);
    assert_eq!(config.execution.output_limit_bytes, 8 * 1024 * 1024);
    assert_eq!(
        config.execution.allowed_executables,
        ["echo", "pwd", "whoami", "friendo-fixture"]
    );
    assert!(
        config.provider().is_err(),
        "offline defaults must not guess a model"
    );
}

#[test]
fn explicit_default_values_retain_their_file_provenance() {
    let root = tempfile::tempdir().unwrap();
    let config = load(root.path(), "autonomy = 'normal'\nfps = 30\ncolor = true").unwrap();
    let fields = &config.inspector()["fields"];
    for name in ["autonomy", "fps", "color"] {
        assert_eq!(
            fields[name]["source"],
            root.path().join("test.toml").display().to_string()
        );
        assert_eq!(fields[name]["precedence"], "CLI > file > built-in");
    }
    assert_eq!(
        fields["effects"]["source"],
        "built-in (or programmatic API override)"
    );
}

#[test]
fn configuration_rejects_unknown_fields_at_each_boundary() {
    let root = tempfile::tempdir().unwrap();
    for text in [
        "unknown = true",
        "[execution]\nallow_shells = true",
        "[providers.fixture]\nmodel = 'explicit'\napi_key = 'must-not-be-inline'",
    ] {
        let error = load(root.path(), text).unwrap_err();
        assert!(
            format!("{error:#}").contains("unknown field"),
            "{text}: {error:#}"
        );
    }
}

#[test]
fn configuration_rejects_invalid_modes_and_numeric_limits() {
    let root = tempfile::tempdir().unwrap();
    for text in [
        "autonomy = 'automatic'",
        "effects = 'flash'",
        "layout = 'floating'",
        "fps = 0",
        "fps = 61",
        "max_turns = 0",
        "max_turns = 65",
        "[execution]\ntimeout_ms = 0",
        "[execution]\ntimeout_ms = 3600001",
        "[execution]\noutput_limit_bytes = 1023",
        "[execution]\noutput_limit_bytes = 268435457",
        "[execution]\ntimeout_ms = -1",
    ] {
        assert!(load(root.path(), text).is_err(), "accepted {text}");
    }
    for text in [
        "fps = 1\nmax_turns = 1\n[execution]\ntimeout_ms = 1\noutput_limit_bytes = 1024",
        "fps = 60\nmax_turns = 64\n[execution]\ntimeout_ms = 3600000\noutput_limit_bytes = 268435456",
    ] {
        load(root.path(), text).unwrap();
    }
}

#[test]
fn remaps_reject_reserved_unknown_multicharacter_and_colliding_bindings() {
    let root = tempfile::tempdir().unwrap();
    for text in [
        "[keys]\nrepair = 'q'",
        "[keys]\nrepair = 'j'",
        "[keys]\nrepair = '2'",
        "[keys]\nrepair = ''",
        "[keys]\nrepair = 'rr'",
        "[keys]\nlaunch = 'x'",
        "[keys]\nrepair = 'x'\nyolo = 'x'",
    ] {
        assert!(load(root.path(), text).is_err(), "accepted {text}");
    }
    assert_eq!(
        load(root.path(), "[keys]\nrepair = 'x'\nyolo = 'z'")
            .unwrap()
            .keys["repair"],
        "x"
    );
}

#[test]
fn executable_admission_requires_exact_paths_and_yolo_does_not_expand_it() {
    let mut config = Config::default();
    let absolute = tempfile::tempdir().unwrap().path().join("echo");
    assert!(config.allows("echo"));
    assert!(!config.allows(absolute.to_str().unwrap()));
    config.autonomy = "yolo".into();
    assert!(!config.allows("sh"));
    assert!(!config.execution.allow_shell);
    config.execution.allowed_executables = vec![absolute.display().to_string()];
    assert!(config.allows(absolute.to_str().unwrap()));
    assert!(!config.allows("echo"));
    config.execution.allowed_executables = vec!["*".into()];
    assert!(config.allows("an-explicitly-opted-in-executable"));
}

#[test]
fn explicitly_missing_and_malformed_configuration_fail() {
    let root = tempfile::tempdir().unwrap();
    assert!(
        Config::load(Some(&root.path().join("missing.toml")))
            .unwrap_err()
            .to_string()
            .contains("does not exist")
    );
    assert!(load(root.path(), "[unclosed").is_err());
    assert!(load(root.path(), "[providers.fixture]\nmodel = ''").is_err());
}
