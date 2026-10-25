use crate::model::{Event, Kind, SCHEMA, State};
use anyhow::{Context, Result, bail};
use std::{
    fs::{self, File, OpenOptions},
    io::{BufRead, BufReader, Read, Write},
    path::{Path, PathBuf},
};
use uuid::Uuid;

pub struct Store {
    pub id: String,
    pub dir: PathBuf,
    pub events: Vec<Event>,
    pub state: State,
    file: File,
    secrets: Vec<String>,
    event_bytes: usize,
}
pub struct Recovery {
    pub events: Vec<Event>,
    pub warnings: Vec<String>,
}
pub fn private_dir(path: &Path) -> Result<()> {
    fs::create_dir_all(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}
pub fn private_file(path: &Path) -> Result<File> {
    let mut opts = OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    Ok(opts.open(path)?)
}
impl Store {
    pub fn create(root: &Path, secrets: Vec<String>) -> Result<Self> {
        private_dir(root)?;
        let root = root.canonicalize()?;
        let id = Uuid::new_v4().to_string();
        let dir = root.join(&id);
        private_dir(&dir)?;
        private_dir(&dir.join("artifacts"))?;
        let file = private_file(&dir.join("events.jsonl"))?;
        Ok(Self {
            id,
            dir,
            events: vec![],
            state: State::default(),
            file,
            secrets,
            event_bytes: 0,
        })
    }
    pub fn redact(&self, text: &str) -> String {
        let mut s = text.to_owned();
        for key in &self.secrets {
            if !key.is_empty() {
                s = s.replace(key, "[REDACTED credential]");
            }
        }
        s
    }
    fn redact_value(&self, value: &mut serde_json::Value) {
        match value {
            serde_json::Value::String(text) => {
                if let Ok(mut nested) = serde_json::from_str::<serde_json::Value>(text)
                    && (nested.is_object() || nested.is_array())
                {
                    let before = nested.clone();
                    self.redact_value(&mut nested);
                    if nested != before {
                        *text = serde_json::to_string(&nested).unwrap_or_default();
                    }
                    return;
                }
                *text = self.redact(text);
            }
            serde_json::Value::Array(values) => {
                for value in values {
                    self.redact_value(value);
                }
            }
            serde_json::Value::Object(values) => {
                for value in values.values_mut() {
                    self.redact_value(value);
                }
            }
            _ => {}
        }
    }
    pub fn append(&mut self, source: &str, causes: Vec<String>, kind: Kind) -> Result<String> {
        if self.events.len() >= 100_000 {
            bail!("run event limit reached; start a new run");
        }
        let event = Event {
            schema: SCHEMA,
            id: Uuid::new_v4().to_string(),
            run_id: self.id.clone(),
            branch_id: "main".into(),
            seq: self.events.len() as u64 + 1,
            timestamp: chrono::Utc::now().to_rfc3339(),
            source: source.into(),
            causes,
            kind,
        };
        let mut value = serde_json::to_value(&event)?;
        self.redact_value(&mut value);
        let text = serde_json::to_string(&value)?;
        if text.len() > 1024 * 1024 {
            bail!("event exceeds 1 MiB; retain large data as artifact");
        }
        if self.event_bytes + text.len() > 64 * 1024 * 1024 {
            bail!("64 MiB event memory budget reached; start a new run");
        }
        let event: Event = serde_json::from_str(&text)?;
        self.file.write_all(text.as_bytes())?;
        self.file.write_all(b"\n")?;
        self.file.sync_data()?;
        self.state.apply(&event);
        let id = event.id.clone();
        self.events.push(event);
        self.event_bytes += text.len();
        Ok(id)
    }
    pub fn artifact(
        &mut self,
        label: &str,
        bytes: &[u8],
        synthetic: bool,
        transformation: &str,
    ) -> Result<(String, String)> {
        let id = Uuid::new_v4().to_string();
        let rel = format!("artifacts/{id}.txt");
        let mut file = private_file(&self.dir.join(&rel))?;
        let original = String::from_utf8_lossy(bytes);
        let text = if let Ok(mut value) = serde_json::from_str::<serde_json::Value>(&original) {
            self.redact_value(&mut value);
            serde_json::to_string_pretty(&value)?
        } else {
            self.redact(&original)
        };
        file.write_all(text.as_bytes())?;
        file.sync_data()?;
        self.append(
            "artifact_store",
            vec![],
            Kind::Artifact {
                id: id.clone(),
                path: rel.clone(),
                label: label.into(),
                bytes: text.len() as u64,
                synthetic,
                transformation: transformation.into(),
            },
        )?;
        Ok((id, rel))
    }
    pub fn read_artifact(&self, rel: &str) -> Result<String> {
        let p = Path::new(rel);
        let full = if p.is_absolute() {
            p.to_path_buf()
        } else {
            self.dir.join(p)
        }
        .canonicalize()?;
        if !full.starts_with(self.dir.canonicalize()?) {
            bail!("artifact reference escapes run directory");
        }
        if fs::metadata(&full)?.len() > 2 * 1024 * 1024 {
            bail!("artifact too large for inspector; inspect file directly");
        }
        Ok(fs::read_to_string(full)?)
    }
    pub fn edge(&mut self, from: &str, to: &str, relation: &str, basis: &str) -> Result<String> {
        let causes = self
            .state
            .nodes
            .get(from)
            .map(|n| vec![n.event.clone()])
            .unwrap_or_default();
        self.append(
            "projection",
            causes,
            Kind::Edge {
                edge: crate::model::Edge {
                    from: from.into(),
                    to: to.into(),
                    relation: relation.into(),
                    basis: basis.into(),
                    event: String::new(),
                },
            },
        )
    }
    pub fn node(&mut self, node: crate::model::Node) -> Result<String> {
        let dependencies = node.dependencies.clone();
        let id = node.id.clone();
        let event = self.append("controller", vec![], Kind::Node { node })?;
        for dependency in dependencies {
            self.edge(&id, &dependency.id, "depends_on", &dependency.basis)?;
        }
        Ok(event)
    }
}
pub fn recover(path: &Path) -> Result<Recovery> {
    let file = File::open(path).context("open journal for read-only review")?;
    let mut reader = BufReader::new(file);
    let mut events = vec![];
    let mut warnings = vec![];
    let mut total_bytes = 0usize;
    loop {
        let mut bytes = vec![];
        let count = reader
            .by_ref()
            .take(1024 * 1024 + 2)
            .read_until(b'\n', &mut bytes)?;
        if count == 0 {
            break;
        }
        total_bytes += count;
        if total_bytes > 64 * 1024 * 1024 + 100_000 {
            bail!("review exceeds the run's 64MiB event-content budget");
        }
        if bytes.len() > 1024 * 1024 + 1 {
            bail!("journal record exceeds 1 MiB");
        }
        if bytes.last() != Some(&b'\n') {
            warnings.push(format!(
                "damaged/truncated tail after {} valid events; tail ignored, file unchanged",
                events.len()
            ));
            break;
        }
        let e: Event = serde_json::from_slice(&bytes)
            .with_context(|| format!("invalid complete record at line {}", events.len() + 1))?;
        if e.schema != SCHEMA || e.seq != events.len() as u64 + 1 {
            bail!("unsupported schema or sequence at {}", e.seq);
        }
        if events
            .first()
            .is_some_and(|first: &Event| first.run_id != e.run_id)
        {
            bail!("mixed run identities");
        }
        events.push(e);
        if events.len() > 100_000 {
            bail!("review event limit exceeded");
        }
    }
    let state = State::at(&events);
    for id in state.running.keys() {
        warnings.push(format!(
            "{id}: outcome unknown; no completion evidence; never automatically replayed"
        ));
    }
    for (id, session) in &state.sessions {
        if session.get("status").and_then(serde_json::Value::as_str) != Some("completed") {
            warnings.push(format!("persistent session {id}: outcome unknown without recorded exit evidence; never automatically reattached or replayed"));
        }
    }
    Ok(Recovery { events, warnings })
}
pub fn resolve_run(root: &Path, id: &str) -> Result<PathBuf> {
    if Uuid::parse_str(id).is_err() {
        bail!("run id must be a UUID returned by friendo runs");
    }
    Ok(root.join(id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    fn two_records(root: &Path) -> (PathBuf, Vec<serde_json::Value>) {
        let mut store = Store::create(root, vec![]).unwrap();
        for text in ["first", "second"] {
            store
                .append("test", vec![], Kind::Status { text: text.into() })
                .unwrap();
        }
        let path = store.dir.join("events.jsonl");
        let records = store
            .events
            .iter()
            .map(|event| serde_json::to_value(event).unwrap())
            .collect();
        (path, records)
    }

    #[test]
    fn complete_invalid_journals_fail_without_modifying_evidence() {
        let root = tempfile::tempdir().unwrap();
        let (path, records) = two_records(root.path());
        let mut cases = vec![b"{broken}\n".to_vec(), vec![0xff, b'\n']];
        for (index, key, value) in [
            (0, "schema", serde_json::json!(SCHEMA + 1)),
            (0, "seq", serde_json::json!(0)),
            (1, "seq", serde_json::json!(3)),
            (1, "seq", serde_json::json!(1)),
            (1, "run_id", serde_json::json!(Uuid::new_v4().to_string())),
            (
                1,
                "kind",
                serde_json::json!({"type":"future_authority_event"}),
            ),
        ] {
            let mut changed = records.clone();
            changed[index][key] = value;
            let mut bytes = vec![];
            for record in changed {
                serde_json::to_writer(&mut bytes, &record).unwrap();
                bytes.push(b'\n');
            }
            cases.push(bytes);
        }
        for bytes in cases {
            fs::write(&path, &bytes).unwrap();
            assert!(
                recover(&path).is_err(),
                "complete invalid record must fail closed"
            );
            assert_eq!(fs::read(&path).unwrap(), bytes);
        }
    }

    #[test]
    fn oversized_records_fail_in_append_and_recovery_without_state_changes() {
        let root = tempfile::tempdir().unwrap();
        let mut store = Store::create(root.path(), vec![]).unwrap();
        store
            .append(
                "test",
                vec![],
                Kind::Status {
                    text: "retained".into(),
                },
            )
            .unwrap();
        let path = store.dir.join("events.jsonl");
        let before = fs::read(&path).unwrap();
        let state = serde_json::to_value(&store.state).unwrap();
        let error = store
            .append(
                "test",
                vec![],
                Kind::Intent {
                    revision: 1,
                    statement: "x".repeat(1024 * 1024),
                    constraints: vec![],
                },
            )
            .unwrap_err();
        assert!(error.to_string().contains("event exceeds 1 MiB"));
        assert_eq!(store.events.len(), 1);
        assert_eq!(serde_json::to_value(&store.state).unwrap(), state);
        assert_eq!(fs::read(&path).unwrap(), before);
        drop(store);
        let mut oversized = vec![b' '; 1024 * 1024 + 1];
        oversized.push(b'\n');
        fs::write(&path, &oversized).unwrap();
        assert!(
            recover(&path)
                .err()
                .unwrap()
                .to_string()
                .contains("record exceeds 1 MiB")
        );
        assert_eq!(fs::read(&path).unwrap(), oversized);
    }

    #[test]
    fn failed_persistence_does_not_apply_a_state_transition() {
        let root = tempfile::tempdir().unwrap();
        let mut store = Store::create(root.path(), vec![]).unwrap();
        let path = store.dir.join("events.jsonl");
        let state = serde_json::to_value(&store.state).unwrap();
        store.file = File::open(&path).unwrap(); // Real read-only descriptor, not a mocked write.
        assert!(
            store
                .append(
                    "user",
                    vec![],
                    Kind::Intent {
                        revision: 1,
                        statement: "must not apply".into(),
                        constraints: vec![]
                    }
                )
                .is_err()
        );
        assert!(store.events.is_empty());
        assert_eq!(serde_json::to_value(&store.state).unwrap(), state);
        assert!(fs::read(&path).unwrap().is_empty());
    }

    #[test]
    fn artifact_reads_reject_escapes_missing_oversized_and_non_text_sources() {
        let root = tempfile::tempdir().unwrap();
        let mut store = Store::create(root.path(), vec![]).unwrap();
        let (_, rel) = store
            .artifact("source", b"exact retained text", false, "none")
            .unwrap();
        assert_eq!(store.read_artifact(&rel).unwrap(), "exact retained text");
        assert_eq!(
            store
                .read_artifact(store.dir.join(&rel).to_str().unwrap())
                .unwrap(),
            "exact retained text"
        );
        let outside = root.path().join("outside.txt");
        fs::write(&outside, b"outside").unwrap();
        for rel in [
            "../outside.txt",
            outside.to_str().unwrap(),
            "artifacts/missing.txt",
        ] {
            assert!(store.read_artifact(rel).is_err(), "accepted {rel}");
        }
        // A lexical prefix is insufficient: a sibling whose name starts with the run ID is outside.
        let sibling = store.dir.with_file_name(format!("{}-sibling", store.id));
        fs::create_dir(&sibling).unwrap();
        fs::write(sibling.join("source.txt"), b"outside").unwrap();
        assert!(
            store
                .read_artifact(sibling.join("source.txt").to_str().unwrap())
                .is_err()
        );
        let large = store.dir.join("artifacts/large.txt");
        File::create(&large)
            .unwrap()
            .set_len(2 * 1024 * 1024 + 1)
            .unwrap();
        assert!(
            store
                .read_artifact("artifacts/large.txt")
                .unwrap_err()
                .to_string()
                .contains("too large")
        );
        fs::write(store.dir.join("artifacts/binary.txt"), [0xff]).unwrap();
        assert!(store.read_artifact("artifacts/binary.txt").is_err());
    }

    #[cfg(unix)]
    #[test]
    fn artifact_symlinks_cannot_escape_the_run() {
        let root = tempfile::tempdir().unwrap();
        let store = Store::create(root.path(), vec![]).unwrap();
        let outside = root.path().join("outside.txt");
        fs::write(&outside, b"outside").unwrap();
        std::os::unix::fs::symlink(outside, store.dir.join("artifacts/link.txt")).unwrap();
        assert!(
            store
                .read_artifact("artifacts/link.txt")
                .unwrap_err()
                .to_string()
                .contains("escapes run directory")
        );
    }
    #[test]
    fn quoted_credentials_are_redacted_before_json_encoding() {
        let root = tempfile::tempdir().unwrap();
        let secret = "a-quoted-\"key\\with\nnewline";
        let mut store = Store::create(root.path(), vec![secret.into()]).unwrap();
        store
            .append(
                "model",
                vec![],
                Kind::Status {
                    text: secret.into(),
                },
            )
            .unwrap();
        let nested = serde_json::json!({"content":serde_json::json!({"key":secret}).to_string()});
        let (_, path) = store
            .artifact(
                "json",
                &serde_json::to_vec(&nested).unwrap(),
                false,
                "known credential redaction",
            )
            .unwrap();
        let decoded: serde_json::Value =
            serde_json::from_str(&store.read_artifact(&path).unwrap()).unwrap();
        let inner: serde_json::Value =
            serde_json::from_str(decoded["content"].as_str().unwrap()).unwrap();
        assert_eq!(inner["key"], "[REDACTED credential]");
        let recovered = recover(&store.dir.join("events.jsonl")).unwrap();
        if let Kind::Status { text } = &recovered.events[0].kind {
            assert_eq!(text, "[REDACTED credential]");
        } else {
            panic!("wrong event");
        }
    }
    #[test]
    fn truncated_tail_and_unknown_outcome() {
        let t = tempfile::tempdir().unwrap();
        let mut s = Store::create(t.path(), vec![]).unwrap();
        s.append(
            "controller",
            vec![],
            Kind::ToolStarted {
                id: "side-effect".into(),
                revision: 1,
                invocation: serde_json::json!({}),
            },
        )
        .unwrap();
        let path = s.dir.join("events.jsonl");
        drop(s);
        let mut f = OpenOptions::new().append(true).open(&path).unwrap();
        f.write_all(b"{\"partial\":").unwrap();
        let r = recover(&path).unwrap();
        assert_eq!(r.events.len(), 1);
        assert_eq!(r.warnings.len(), 2);
        assert!(r.warnings[1].contains("outcome unknown"));
        let mut bytes = String::new();
        File::open(path)
            .unwrap()
            .read_to_string(&mut bytes)
            .unwrap();
        assert!(bytes.ends_with(":"));
    }
    #[test]
    fn credentials_redacted_and_run_creation_unique() {
        let t = tempfile::tempdir().unwrap();
        let mut s = Store::create(t.path(), vec!["test-secret-123".into()]).unwrap();
        s.append(
            "model",
            vec![],
            Kind::Status {
                text: "test-secret-123".into(),
            },
        )
        .unwrap();
        let (_, p) = s
            .artifact(
                "context",
                b"test-secret-123",
                false,
                "credential replacement",
            )
            .unwrap();
        assert!(!s.read_artifact(&p).unwrap().contains("test-secret-123"));
        assert!(
            !std::fs::read_to_string(s.dir.join("events.jsonl"))
                .unwrap()
                .contains("test-secret-123")
        );
        assert_ne!(s.id, Store::create(t.path(), vec![]).unwrap().id);
    }
}
