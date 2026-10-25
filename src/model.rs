use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

pub const SCHEMA: u32 = 1;
pub const MCW_REVISION: &str = "8365d220f2676f248c934e20f23e427e01cf3ce8";
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureMode {
    Drift,
    AsymmetricStateAdvancement,
    FalseAlignment,
    Overcompression,
    ConstraintOpacity,
    RepairSuppression,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssessmentBasis {
    MechanicalCheck,
    ModelAssessment,
    HumanReport,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Event {
    pub schema: u32,
    pub id: String,
    pub run_id: String,
    pub branch_id: String,
    pub seq: u64,
    pub timestamp: String,
    pub source: String,
    pub causes: Vec<String>,
    pub kind: Kind,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Kind {
    Assessment {
        mode: FailureMode,
        basis: AssessmentBasis,
        possible: bool,
        revision: u64,
        text: String,
    },
    Acknowledged {
        revision: u64,
        text: String,
    },
    Constraints {
        revision: u64,
        items: Vec<String>,
    },
    Started {
        demo: bool,
        config: Value,
        source_revision: String,
    },
    Intent {
        revision: u64,
        statement: String,
        constraints: Vec<String>,
    },
    Correction {
        revision: u64,
        statement: String,
        operation: String,
        target: Option<String>,
    },
    Interpretation {
        revision: u64,
        text: String,
        basis: String,
    },
    Node {
        node: Node,
    },
    Edge {
        edge: Edge,
    },
    Artifact {
        id: String,
        path: String,
        label: String,
        bytes: u64,
        synthetic: bool,
        transformation: String,
    },
    ModelRequested {
        id: String,
        revision: u64,
        context: String,
    },
    ModelFinished {
        id: String,
        text: String,
        response_artifact: String,
        usage: Option<Value>,
    },
    ToolStarted {
        id: String,
        revision: u64,
        invocation: Value,
    },
    ToolFinished {
        id: String,
        result: Value,
    },
    Delivered {
        revision: u64,
        request: String,
    },
    Refreshed {
        node: String,
        revision: u64,
        basis: String,
    },
    Check {
        id: String,
        revision: u64,
        passed: bool,
        condition: String,
        evidence: Vec<String>,
    },
    Pin {
        node: String,
        text: String,
        policy: String,
    },
    Autonomy {
        mode: String,
        boundary: String,
    },
    Conflict {
        text: String,
        basis: String,
        unresolved: bool,
    },
    Assumption {
        text: String,
        basis: String,
    },
    Repair {
        operation: String,
        revision: u64,
        receipt: String,
    },
    Restored {
        revision: u64,
        conditions: Vec<String>,
        evidence: Vec<String>,
    },
    CancelRequested {
        reason: String,
    },
    Status {
        text: String,
    },
    Failure {
        text: String,
        outcome: String,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Dependency {
    pub id: String,
    pub basis: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Node {
    pub id: String,
    pub label: String,
    pub kind: String,
    pub lens: String,
    pub revision: Option<u64>,
    pub dependencies: Vec<Dependency>,
    pub stale: bool,
    pub synthetic: bool,
    pub detail: String,
    #[serde(default)]
    pub event: String,
}
impl Node {
    pub fn new(
        id: &str,
        label: &str,
        kind: &str,
        lens: &str,
        revision: Option<u64>,
        detail: &str,
    ) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            kind: kind.into(),
            lens: lens.into(),
            revision,
            dependencies: vec![],
            stale: false,
            synthetic: false,
            detail: detail.into(),
            event: String::new(),
        }
    }
    pub fn depends(mut self, id: &str) -> Self {
        self.dependencies.push(Dependency {
            id: id.into(),
            basis: "declared".into(),
        });
        self
    }
    pub fn synthetic(mut self) -> Self {
        self.synthetic = true;
        self
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Edge {
    pub from: String,
    pub to: String,
    pub relation: String,
    pub basis: String,
    #[serde(default)]
    pub event: String,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct State {
    pub revision: u64,
    pub goal: String,
    pub constraints: Vec<String>,
    pub interpretation: String,
    pub interpretation_revision: u64,
    pub delivered_revision: u64,
    pub demo: bool,
    pub autonomy: String,
    pub nodes: BTreeMap<String, Node>,
    pub edges: Vec<Edge>,
    pub pins: BTreeMap<String, String>,
    pub status: String,
    pub receipt: String,
    pub unresolved: bool,
    pub checks: BTreeMap<String, bool>,
    pub running: BTreeMap<String, Value>,
    pub sessions: BTreeMap<String, Value>,
    pub artifacts: BTreeMap<String, String>,
    pub transcript: String,
    pub current_answer: String,
    pub assessments: Vec<Value>,
    pub acknowledged_revision: Option<u64>,
    pub last_context: String,
    pub last_event: String,
    pub seq: u64,
    pub restored: bool,
}
impl State {
    pub fn apply(&mut self, event: &Event) {
        self.seq = event.seq;
        self.last_event = event.id.clone();
        match &event.kind {
            Kind::Assessment { .. } => {
                self.assessments
                    .push(serde_json::to_value(event).unwrap_or_default());
            }
            Kind::Acknowledged { revision, .. } if event.source == "user" => {
                self.acknowledged_revision = Some(*revision)
            }
            Kind::Constraints { items, .. } if event.source == "user" => {
                self.constraints = items.clone()
            }
            Kind::Started { demo, .. } => {
                self.demo = *demo;
                self.autonomy = "normal".into();
            }
            Kind::Intent {
                revision,
                statement,
                constraints,
            } => {
                self.revision = *revision;
                self.goal = statement.clone();
                self.constraints = constraints.clone();
            }
            Kind::Correction {
                revision,
                statement,
                ..
            } => {
                self.revision = *revision;
                self.goal = statement.clone();
                self.restored = false;
                self.checks.clear();
                for n in self.nodes.values_mut() {
                    if n.revision.is_some_and(|r| r < *revision) {
                        n.stale = true;
                    }
                }
                // Follow explicit transitive dependencies; independent nodes retain availability.
                loop {
                    let stale: Vec<_> = self
                        .nodes
                        .values()
                        .filter(|n| n.stale)
                        .map(|n| n.id.clone())
                        .collect();
                    let mut changed = false;
                    for n in self.nodes.values_mut() {
                        if !n.stale && n.dependencies.iter().any(|d| stale.contains(&d.id)) {
                            n.stale = true;
                            changed = true;
                        }
                    }
                    if !changed {
                        break;
                    }
                }
                self.status = "CORRECTION RECORDED / dependent work stale".into();
            }
            Kind::Interpretation { revision, text, .. } => {
                self.interpretation = text.clone();
                self.interpretation_revision = *revision;
            }
            Kind::Node { node } => {
                let mut n = node.clone();
                n.event = event.id.clone();
                self.edges
                    .retain(|edge| !(edge.from == n.id && edge.relation == "depends_on"));
                self.nodes.insert(n.id.clone(), n);
            }
            Kind::Edge { edge } => {
                let mut e = edge.clone();
                e.event = event.id.clone();
                self.edges.push(e);
            }
            Kind::Artifact { id, path, .. } => {
                self.artifacts.insert(id.clone(), path.clone());
            }
            Kind::ModelRequested { id, context, .. } => {
                self.last_context = context.clone();
                self.status = format!("STREAMING / {id}");
            }
            Kind::ModelFinished { text, .. } => {
                self.current_answer = text.clone();
                self.transcript.push_str(text);
                self.transcript.push('\n');
                if self.transcript.len() > 65536 {
                    let cut = self
                        .transcript
                        .floor_char_boundary(self.transcript.len() - 65536);
                    self.transcript.drain(..cut);
                }
            }
            Kind::ToolStarted { id, invocation, .. } => {
                self.running.insert(id.clone(), invocation.clone());
                self.status = format!("EXECUTING / {id}");
            }
            Kind::ToolFinished { id, result } => {
                self.running.remove(id);
                if let Some(sid) = result.get("session_id").and_then(Value::as_str) {
                    let entry = self
                        .sessions
                        .entry(sid.into())
                        .or_insert(serde_json::json!({}));
                    if let (Some(old), Some(new)) = (entry.as_object_mut(), result.as_object()) {
                        old.extend(new.clone());
                    }
                }
            }
            Kind::Delivered { revision, .. } => self.delivered_revision = *revision,
            Kind::Refreshed { node, revision, .. } => {
                if let Some(n) = self.nodes.get_mut(node) {
                    n.revision = Some(*revision);
                    n.stale = false;
                }
            }
            Kind::Check { id, passed, .. } => {
                self.checks.insert(id.clone(), *passed);
            }
            Kind::Pin { node, text, .. } => {
                self.pins.insert(node.clone(), text.clone());
            }
            Kind::Autonomy { mode, .. }
                if event.source == "user" || event.source == "user_config" =>
            {
                self.autonomy = mode.clone()
            }
            Kind::Conflict { unresolved, .. } => self.unresolved = *unresolved,
            Kind::Repair { receipt, .. } => self.receipt = receipt.clone(),
            Kind::Restored { revision, .. } if *revision == self.revision => {
                self.restored = true;
                self.receipt = "LINK RESTORED / recorded revision and declared checks agree".into();
            }
            Kind::CancelRequested { reason } => {
                self.status = format!("CANCELLATION REQUESTED / {reason}")
            }
            Kind::Status { text } => self.status = text.clone(),
            Kind::Failure { text, outcome } => self.status = format!("{outcome} / {text}"),
            _ => {}
        }
    }
    pub fn at(events: &[Event]) -> Self {
        let mut s = Self::default();
        for e in events {
            s.apply(e);
        }
        s
    }
    pub fn trace(&self, id: &str, downstream: bool) -> Vec<String> {
        let mut found = vec![id.to_owned()];
        loop {
            let mut more = vec![];
            for e in &self.edges {
                let inverse = matches!(e.relation.as_str(), "depends_on" | "supersedes");
                let (a, b) = if downstream != inverse {
                    (&e.from, &e.to)
                } else {
                    (&e.to, &e.from)
                };
                if found.contains(a) && !found.contains(b) && !more.contains(b) {
                    more.push(b.clone());
                }
            }
            if more.is_empty() {
                break;
            }
            found.extend(more);
        }
        found
    }
    pub fn pending_checks(&self) -> usize {
        self.checks.values().filter(|p| !**p).count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn ev(kind: Kind, source: &str) -> Event {
        Event {
            schema: 1,
            id: "e".into(),
            run_id: "r".into(),
            branch_id: "main".into(),
            seq: 1,
            timestamp: String::new(),
            source: source.into(),
            causes: vec![],
            kind,
        }
    }
    #[test]
    fn revision_stales_dependencies_preserves_evidence_and_independent_work() {
        let mut s = State::default();
        for node in [
            Node::new("plan", "p", "plan", "coordination", Some(1), ""),
            Node::new("answer", "a", "claim", "evidence", None, "").depends("plan"),
            Node::new("artifact", "old bytes", "artifact", "evidence", None, ""),
            Node::new("independent", "i", "process", "runtime", None, ""),
        ] {
            s.apply(&ev(Kind::Node { node }, "controller"));
        }
        s.apply(&ev(
            Kind::Correction {
                revision: 2,
                statement: "new".into(),
                operation: "synchronization".into(),
                target: None,
            },
            "user",
        ));
        assert!(s.nodes["answer"].stale);
        assert!(s.nodes["plan"].stale);
        assert!(!s.nodes["artifact"].stale);
        assert!(!s.nodes["independent"].stale);
    }
    #[test]
    fn untrusted_content_cannot_enable_yolo() {
        let mut s = State {
            autonomy: "normal".into(),
            ..State::default()
        };
        s.apply(&ev(
            Kind::Autonomy {
                mode: "yolo".into(),
                boundary: "now".into(),
            },
            "model",
        ));
        assert_eq!(s.autonomy, "normal");
    }
}
