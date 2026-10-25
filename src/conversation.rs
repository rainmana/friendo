//! Read-only conversation cards derived from the selected journal prefix.

use crate::model::{Event, Kind};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CardKind {
    User,
    Correction,
    Assistant,
    Tool,
    Notice,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ConversationCard {
    pub id: String,
    pub kind: CardKind,
    pub source_event: String,
    pub outcome_event: Option<String>,
    pub revision: Option<u64>,
    pub text: String,
    pub invocation: Option<Value>,
    pub result: Option<Value>,
}

/// Project only the supplied prefix; a card cannot borrow a later outcome.
///
/// Source event IDs let the renderer inspect original attribution. Tool results
/// remain observations rather than a derived claim of successful execution.
pub fn project(events: &[Event]) -> Vec<ConversationCard> {
    let mut cards: Vec<ConversationCard> = Vec::new();
    let mut pending_tools = BTreeMap::new();
    let mut request_revisions = BTreeMap::new();
    for event in events {
        match &event.kind {
            Kind::Intent {
                revision,
                statement,
                ..
            } => {
                cards.push(card(event, CardKind::User, statement, Some(*revision)));
            }
            Kind::Correction {
                revision,
                statement,
                ..
            } => {
                cards.push(card(
                    event,
                    CardKind::Correction,
                    statement,
                    Some(*revision),
                ));
            }
            Kind::ModelRequested { id, revision, .. } => {
                request_revisions.insert(id.as_str(), *revision);
            }
            Kind::ModelFinished { id, text, .. } if !text.is_empty() => {
                cards.push(card(
                    event,
                    CardKind::Assistant,
                    text,
                    request_revisions.get(id.as_str()).copied(),
                ));
            }
            Kind::ToolStarted {
                id,
                revision,
                invocation,
            } => {
                let mut tool = card(event, CardKind::Tool, id, Some(*revision));
                tool.id = id.clone();
                tool.invocation = Some(invocation.clone());
                pending_tools.insert(id.as_str(), cards.len());
                cards.push(tool);
            }
            Kind::ToolFinished { id, result } => {
                if let Some(index) = pending_tools.remove(id.as_str()) {
                    cards[index].result = Some(result.clone());
                    cards[index].outcome_event = Some(event.id.clone());
                } else {
                    // A result cannot invent a start or satisfy a future start.
                    let mut notice = card(
                        event,
                        CardKind::Notice,
                        "Recorded result has no matching start; inspect evidence",
                        None,
                    );
                    notice.outcome_event = Some(event.id.clone());
                    notice.result = Some(result.clone());
                    cards.push(notice);
                }
            }
            Kind::Failure { text, .. } | Kind::Conflict { text, .. } => {
                cards.push(card(event, CardKind::Notice, text, None));
            }
            _ => {}
        }
    }
    cards
}

fn card(event: &Event, kind: CardKind, text: &str, revision: Option<u64>) -> ConversationCard {
    ConversationCard {
        id: event.id.clone(),
        kind,
        source_event: event.id.clone(),
        outcome_event: None,
        revision,
        text: text.into(),
        invocation: None,
        result: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Kind;
    use serde_json::json;

    fn event(id: &str, source: &str, kind: Kind) -> Event {
        Event {
            schema: crate::model::SCHEMA,
            id: id.into(),
            run_id: "recorded-run".into(),
            branch_id: "main".into(),
            seq: 1,
            timestamp: "2026-10-25T20:00:00Z".into(),
            source: source.into(),
            causes: vec![],
            kind,
        }
    }

    fn requested(id: &str, request: &str, revision: u64) -> Event {
        event(
            id,
            "controller",
            Kind::ModelRequested {
                id: request.into(),
                revision,
                context: "artifacts/request.json".into(),
            },
        )
    }

    fn answered(id: &str, request: &str, text: &str) -> Event {
        event(
            id,
            "model",
            Kind::ModelFinished {
                id: request.into(),
                text: text.into(),
                response_artifact: "artifacts/response.json".into(),
                usage: None,
            },
        )
    }

    fn started(id: &str, tool: &str, revision: u64) -> Event {
        event(
            id,
            "controller",
            Kind::ToolStarted {
                id: tool.into(),
                revision,
                invocation: json!({"executable":"whoami","args":[]}),
            },
        )
    }

    fn finished(id: &str, tool: &str, result: Value) -> Event {
        event(
            id,
            "tool",
            Kind::ToolFinished {
                id: tool.into(),
                result,
            },
        )
    }

    #[test]
    fn exact_user_correction_and_answer_text_keep_recorded_order_and_sources() {
        let events = vec![
            event(
                "intent-1",
                "user",
                Kind::Intent {
                    revision: 1,
                    statement: "  whoami?\nKeep café + 🦀 exact.  ".into(),
                    constraints: vec!["No shell".into()],
                },
            ),
            requested("requested-1", "request-1", 1),
            answered("answer-1", "request-1", "First answer.\n"),
            event(
                "correction-2",
                "user",
                Kind::Correction {
                    revision: 2,
                    statement: "  Correction:\ncurrent account only.  ".into(),
                    operation: "disambiguation".into(),
                    target: None,
                },
            ),
            requested("requested-2", "request-2", 2),
            answered("answer-2", "request-2", "Revised answer.\n"),
        ];
        let cards = project(&events);
        assert_eq!(
            cards
                .iter()
                .map(|card| (
                    &card.kind,
                    card.text.as_str(),
                    card.source_event.as_str(),
                    card.revision
                ))
                .collect::<Vec<_>>(),
            vec![
                (
                    &CardKind::User,
                    "  whoami?\nKeep café + 🦀 exact.  ",
                    "intent-1",
                    Some(1)
                ),
                (&CardKind::Assistant, "First answer.\n", "answer-1", Some(1)),
                (
                    &CardKind::Correction,
                    "  Correction:\ncurrent account only.  ",
                    "correction-2",
                    Some(2)
                ),
                (
                    &CardKind::Assistant,
                    "Revised answer.\n",
                    "answer-2",
                    Some(2)
                ),
            ]
        );
        assert_eq!(
            cards
                .iter()
                .map(|card| card.id.as_str())
                .collect::<Vec<_>>(),
            ["intent-1", "answer-1", "correction-2", "answer-2"]
        );
    }

    #[test]
    fn assistant_revision_uses_matching_request_instead_of_latest_intent() {
        let events = vec![
            requested("old-request", "request-3", 3),
            event(
                "new-intent",
                "user",
                Kind::Intent {
                    revision: 4,
                    statement: "New direction".into(),
                    constraints: vec![],
                },
            ),
            answered("old-answer", "request-3", "From the older request"),
            answered("unmatched-answer", "unmatched-request", "Unknown revision"),
        ];
        let cards = project(&events);
        assert_eq!(cards.len(), 3);
        assert_eq!(cards[1].revision, Some(3));
        assert_eq!(cards[2].revision, None);
        assert_eq!(cards[2].source_event, "unmatched-answer");
    }

    #[test]
    fn repeated_answers_remain_distinct_and_empty_completions_add_no_message() {
        let cards = project(&[
            answered("answer-a", "request-a", "Same answer"),
            answered("empty-answer", "request-b", ""),
            answered("answer-c", "request-c", "Same answer"),
        ]);
        assert_eq!(
            cards
                .iter()
                .map(|card| (card.id.as_str(), card.text.as_str()))
                .collect::<Vec<_>>(),
            [("answer-a", "Same answer"), ("answer-c", "Same answer")]
        );
    }

    #[test]
    fn interleaved_tool_outcomes_pair_by_identity_without_reordering_cards() {
        let cards = project(&[
            started("start-a", "tool-a", 2),
            started("start-b", "tool-b", 3),
            answered("between-tools", "request-z", "Still waiting"),
            finished(
                "finish-b",
                "tool-b",
                json!({"exit_code":1,"termination":"exited","stderr":"denied"}),
            ),
            finished(
                "finish-a",
                "tool-a",
                json!({"exit_code":0,"termination":"exited","stdout":"alec\n"}),
            ),
        ]);
        assert_eq!(
            cards
                .iter()
                .map(|card| card.id.as_str())
                .collect::<Vec<_>>(),
            ["tool-a", "tool-b", "between-tools"]
        );
        assert_eq!(cards[0].kind, CardKind::Tool);
        assert_eq!(cards[0].source_event, "start-a");
        assert_eq!(cards[0].outcome_event.as_deref(), Some("finish-a"));
        assert_eq!(cards[0].revision, Some(2));
        assert_eq!(
            cards[0].invocation,
            Some(json!({"executable":"whoami","args":[]}))
        );
        assert_eq!(
            cards[0].result,
            Some(json!({"exit_code":0,"termination":"exited","stdout":"alec\n"}))
        );
        assert_eq!(cards[1].source_event, "start-b");
        assert_eq!(cards[1].outcome_event.as_deref(), Some("finish-b"));
        assert_eq!(cards[1].revision, Some(3));
        assert_eq!(
            cards[1].result,
            Some(json!({"exit_code":1,"termination":"exited","stderr":"denied"}))
        );
    }

    #[test]
    fn historical_prefix_never_borrows_a_future_completion() {
        let events = [
            started("start", "tool", 1),
            finished(
                "finish",
                "tool",
                json!({"termination":"capture_incomplete","exit_code":null}),
            ),
        ];
        let prefix = project(&events[..1]);
        assert_eq!(prefix.len(), 1);
        assert_eq!(prefix[0].kind, CardKind::Tool);
        assert_eq!(prefix[0].result, None);
        assert_eq!(prefix[0].outcome_event, None);
        let completed = project(&events);
        assert_eq!(
            completed[0].result,
            Some(json!({"termination":"capture_incomplete","exit_code":null}))
        );
        assert_eq!(completed[0].outcome_event.as_deref(), Some("finish"));
    }

    #[test]
    fn orphan_outcome_remains_an_attributed_notice_and_cannot_complete_a_later_start() {
        let cards = project(&[
            finished("orphan", "tool", json!({"exit_code":0,"stdout":"earlier"})),
            started("later-start", "tool", 7),
        ]);
        assert_eq!(cards.len(), 2);
        assert_eq!(cards[0].kind, CardKind::Notice);
        assert_eq!(cards[0].source_event, "orphan");
        assert_eq!(cards[0].outcome_event.as_deref(), Some("orphan"));
        assert_eq!(cards[0].invocation, None);
        assert_eq!(
            cards[0].result,
            Some(json!({"exit_code":0,"stdout":"earlier"}))
        );
        assert_eq!(cards[1].id, "tool");
        assert_eq!(cards[1].kind, CardKind::Tool);
        assert_eq!(cards[1].source_event, "later-start");
        assert_eq!(cards[1].result, None);
        assert_eq!(cards[1].outcome_event, None);
    }

    #[test]
    fn explicit_failures_and_conflicts_are_notices_without_inventing_conversation() {
        let cards = project(&[
            event(
                "policy",
                "user_config",
                Kind::Autonomy {
                    mode: "normal".into(),
                    boundary: "next dispatch".into(),
                },
            ),
            event(
                "status",
                "controller",
                Kind::Status {
                    text: "Ready".into(),
                },
            ),
            event(
                "conflict",
                "user",
                Kind::Conflict {
                    text: "I meant this account".into(),
                    basis: "human_report".into(),
                    unresolved: true,
                },
            ),
            event(
                "failure",
                "controller",
                Kind::Failure {
                    text: "Capture incomplete".into(),
                    outcome: "unknown".into(),
                },
            ),
        ]);
        assert_eq!(
            cards
                .iter()
                .map(|card| (&card.kind, card.text.as_str(), card.source_event.as_str()))
                .collect::<Vec<_>>(),
            [
                (&CardKind::Notice, "I meant this account", "conflict"),
                (&CardKind::Notice, "Capture incomplete", "failure")
            ]
        );
        assert!(
            cards
                .iter()
                .all(|card| card.invocation.is_none() && card.result.is_none())
        );
    }
}
