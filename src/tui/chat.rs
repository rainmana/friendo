use super::*;
use crate::conversation::{self, CardKind, ConversationCard};
use serde_json::Value;

pub(super) fn render(f: &mut Frame, state: &State, events: &[Event], view: &View) {
    let area = f.area();
    f.render_widget(
        Block::default().style(Style::default().bg(INK).fg(WHITE)),
        area,
    );
    if area.width < 38 || area.height < 14 {
        f.render_widget(
            Paragraph::new("friendo / CHAT\nResize to 38 × 14\nCtrl-C stop\nq quit outside input")
                .style(Style::default().fg(CYAN)),
            area,
        );
        monochrome(f, area, view);
        return;
    }
    let footer_height = if area.width < 100 { 4 } else { 2 };
    let rows = Layout::vertical([
        Constraint::Length(4),
        Constraint::Min(3),
        Constraint::Length(4),
        Constraint::Length(footer_height),
    ])
    .split(area);
    header(f, rows[0], state, events, view);
    let cards = conversation::project(events);
    if area.width >= 110 && area.height >= 25 {
        let columns = Layout::horizontal([Constraint::Percentage(62), Constraint::Percentage(38)])
            .split(rows[1]);
        conversation(f, columns[0], &cards, events, view);
        let side = Layout::vertical([Constraint::Percentage(55), Constraint::Percentage(45)])
            .split(columns[1]);
        live_map(f, side[0], state, events, view);
        understanding(f, side[1], state, &cards, events, view, false);
    } else {
        let body = Layout::vertical([
            Constraint::Length(if rows[1].height >= 10 { 6 } else { 3 }),
            Constraint::Min(2),
        ])
        .split(rows[1]);
        let instruments =
            Layout::horizontal([Constraint::Percentage(57), Constraint::Percentage(43)])
                .split(body[0]);
        live_map(f, instruments[0], state, events, view);
        understanding(f, instruments[1], state, &cards, events, view, true);
        conversation(f, body[1], &cards, events, view);
    }
    composer(f, rows[2], state, view);
    footer(f, rows[3], area.width, view);
    if view.help {
        overlay(
            f,
            " CHAT / FIELD MANUAL · Esc close ",
            "CHAT\nEnter sends while typing. q/y/r are ordinary text in the composer.\nEsc leaves text input; i returns to it. Ctrl-C interrupts live work.\nF2 switches Chat/Expert without changing the run, draft, history, or autonomy.\nF3 shows selected-card details. F4 opens Correct direction. F5 opens actual context.\nF6 focuses the map; arrows pan, Tab lens, j/k select, Enter inspects.\nOutside input: arrows select cards; Enter expands tools; PgUp/PgDn scroll.\n[ ] steps recorded history. Review is read-only; uncertain actions never rerun.\nq quits outside input; ? opens help.\n\nUNDERSTANDING\nUser instructions, model interpretations, and executor observations remain distinct.\nOlder interpretations are marked STALE. Provider-internal reasoning is unavailable.\nTool completion alone does not prove exit success or complete output capture.\n\nEXPERT CONTROLS (configured remaps apply)\nThe full console, lenses, provenance, and Rewire remain available with F2.",
            view.scroll,
            CYAN,
        );
    }
    if view.inspect {
        let detail = if view.retained.is_empty() {
            "No retained detail selected."
        } else {
            &view.retained
        };
        overlay(
            f,
            " DETAILS / retained content · Esc close ",
            &sanitize(detail),
            view.scroll,
            CYAN,
        );
    }
    if let Some(correction) = &view.repair {
        correction_editor(f, state, view, correction);
    }
    monochrome(f, area, view);
}

fn header(f: &mut Frame, area: Rect, state: &State, events: &[Event], view: &View) {
    let config = events.iter().find_map(|event| match &event.kind {
        Kind::Started { config, .. } => Some(config),
        _ => None,
    });
    let alias = config
        .and_then(|value| value.pointer("/fields/active_provider/value"))
        .and_then(Value::as_str)
        .unwrap_or("none");
    let model = config
        .and_then(|value| value.pointer("/fields/providers/value"))
        .and_then(|value| value.get(alias))
        .and_then(|value| value.get("model"))
        .and_then(Value::as_str)
        .unwrap_or(if state.demo {
            "deterministic fixture"
        } else {
            "unconfigured"
        });
    let shell = match config
        .and_then(|value| value.pointer("/fields/execution/value/allow_shell"))
        .and_then(Value::as_bool)
    {
        Some(true) => "ON",
        Some(false) => "OFF",
        None => "?",
    };
    let admitted = config
        .and_then(|value| value.pointer("/fields/execution/value/allowed_executables"))
        .and_then(Value::as_array)
        .map(Vec::len);
    let review = if view.read_only || view.review.is_some() {
        "READ-ONLY REVIEW"
    } else {
        "LIVE"
    };
    let mode = if state.autonomy == "yolo" {
        "YOLO"
    } else {
        "NORMAL"
    };
    let title = Line::from(vec![
        Span::styled(" ◇ friendo ", Style::default().fg(INK).bg(CYAN).bold()),
        Span::styled(" CHAT / ", Style::default().fg(PINK).bold()),
        Span::styled(
            if state.demo {
                "OFFLINE DEMO · SYNTHETIC "
            } else {
                "CONTAINMENT IS A METAPHOR "
            },
            Style::default().fg(DIM),
        ),
    ]);
    let lines = vec![
        Line::from(vec![
            Span::styled(
                format!(" AUTONOMY: {mode} "),
                Style::default()
                    .fg(if mode == "YOLO" { PINK } else { CYAN })
                    .bold(),
            ),
            Span::raw(format!(
                "│ r{} │ {review} │ FX {}",
                state.revision, view.effects
            )),
        ]),
        Line::from(Span::styled(
            sanitize(&format!(
                " {alias} / {model} · SHELL {shell} · {} admitted · COST ?",
                admitted
                    .map(|count| count.to_string())
                    .unwrap_or_else(|| "?".into())
            )),
            Style::default().fg(DIM),
        )),
    ];
    f.render_widget(
        Paragraph::new(lines).block(panel(title, if state.unresolved { AMBER } else { CYAN })),
        area,
    );
}

fn live_map(f: &mut Frame, area: Rect, state: &State, events: &[Event], view: &View) {
    let rows = Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).split(area);
    f.render_widget(
        Paragraph::new(if view.map_focus {
            " LIVE MAP / F6 chat "
        } else {
            " LIVE MAP / F6 focus "
        })
        .style(Style::default().fg(CYAN)),
        rows[0],
    );
    let mut map_view = view.clone();
    map_view.compact_map = true;
    render_map(f, rows[1], state, events, &map_view);
}

fn conversation(
    f: &mut Frame,
    area: Rect,
    cards: &[ConversationCard],
    events: &[Event],
    view: &View,
) {
    let block = panel(" CONVERSATION / CHAT ", PINK);
    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.is_empty() {
        return;
    }
    let mut lines = Vec::new();
    let mut selection_start = None;
    for card in cards {
        if view.chat_selected.as_deref() == Some(&card.id) {
            selection_start = Some(lines.len());
        }
        let card_lines = card_lines(card, events, view);
        lines.extend(wrap_lines(card_lines, inner.width));
    }
    if cards.is_empty() {
        lines.extend(wrap_lines(
            vec![
                Line::from("No messages recorded."),
                Line::from("Type a task below. The map shows recorded work."),
            ],
            inner.width,
        ));
    }
    if !view.read_only && view.review.is_none() && !view.preview.is_empty() {
        lines.push(Line::from(Span::styled(
            " ◇ STREAM PREVIEW / INCOMPLETE ",
            Style::default().fg(AMBER).underlined(),
        )));
        let preview = bounded(&view.preview, 8192);
        lines.extend(wrap_lines(text_lines(&preview, WHITE), inner.width));
    }
    let end = lines.len().saturating_sub(usize::from(view.chat_scroll));
    let mut start = end.saturating_sub(usize::from(inner.height));
    if view.chat_scroll == 0
        && view.composer.is_none()
        && let Some(selected) = selection_start
    {
        start = start.min(selected);
    }
    let visible: Vec<_> = lines
        .into_iter()
        .skip(start)
        .take(usize::from(inner.height))
        .collect();
    f.render_widget(Paragraph::new(visible), inner);
}

fn card_lines(card: &ConversationCard, events: &[Event], view: &View) -> Vec<Line<'static>> {
    let source = events
        .iter()
        .find(|event| event.id == card.source_event)
        .map(|event| event.source.as_str())
        .unwrap_or("unavailable");
    let selected = view.chat_selected.as_deref() == Some(&card.id);
    let marker = if selected { "▶ " } else { "  " };
    let revision = card
        .revision
        .map(|revision| format!(" / r{revision}"))
        .unwrap_or_default();
    let (label, color) = match card.kind {
        CardKind::User => ("⌁ YOU", AMBER),
        CardKind::Correction => ("⌁ CORRECTION", AMBER),
        CardKind::Assistant => ("◇ FRIENDO", CYAN),
        CardKind::Tool => ("▣ TOOL", CYAN),
        CardKind::Notice => ("! NOTICE", AMBER),
    };
    let heading = if card.kind == CardKind::Tool {
        format!(
            "{marker}▣ {} / {}",
            command_label(card),
            tool_state(card.result.as_ref(), view).0
        )
    } else {
        format!("{marker}{label} / {}{revision}", source.to_uppercase())
    };
    let color = if selected {
        PINK
    } else if card.kind == CardKind::Tool {
        tool_state(card.result.as_ref(), view).1
    } else {
        color
    };
    let mut lines = vec![Line::from(Span::styled(
        sanitize(&heading),
        Style::default().fg(color).bold().underlined(),
    ))];
    if card.kind == CardKind::Tool {
        if let Some(result) = &card.result {
            append_streams(&mut lines, result);
        } else {
            lines.push(Line::from(Span::styled(
                if view.read_only || view.review.is_some() {
                    "  No completion recorded at this history position."
                } else {
                    "  Awaiting an executor observation."
                },
                Style::default().fg(DIM),
            )));
        }
        if view.chat_expanded.contains(&card.id) {
            if let Some(invocation) = &card.invocation {
                lines.push(Line::from(Span::styled(
                    "  INVOCATION / recorded JSON",
                    Style::default().fg(DIM),
                )));
                lines.extend(text_lines(
                    &bounded(
                        &serde_json::to_string_pretty(invocation).unwrap_or_default(),
                        6000,
                    ),
                    WHITE,
                ));
            }
            if let Some(result) = &card.result {
                lines.push(Line::from(Span::styled(
                    "  RESULT / recorded JSON",
                    Style::default().fg(DIM),
                )));
                lines.extend(text_lines(
                    &bounded(
                        &serde_json::to_string_pretty(result).unwrap_or_default(),
                        6000,
                    ),
                    WHITE,
                ));
            }
        }
    } else {
        lines.extend(text_lines(&bounded(&card.text, 8192), WHITE));
        if let Some(result) = &card.result {
            append_streams(&mut lines, result);
        }
    }
    if view.chat_expanded.contains(&card.id) {
        let outcome = card.outcome_event.as_deref().unwrap_or("not recorded");
        lines.push(Line::from(Span::styled(
            format!("  SOURCE {} · OUTCOME {outcome}", card.source_event),
            Style::default().fg(DIM),
        )));
    }
    lines.push(Line::default());
    lines
}

fn command_label(card: &ConversationCard) -> String {
    let Some(invocation) = &card.invocation else {
        return card.id.clone();
    };
    let arguments = invocation.get("arguments").unwrap_or(invocation);
    let command = arguments
        .get("executable")
        .or_else(|| arguments.get("shell"))
        .and_then(Value::as_str)
        .or_else(|| {
            invocation
                .get("tool")
                .or_else(|| invocation.get("name"))
                .and_then(Value::as_str)
        })
        .unwrap_or("recorded tool");
    let args = arguments
        .get("args")
        .filter(|value| value.as_array().is_some_and(|args| !args.is_empty()))
        .map(|value| format!(" {value}"))
        .unwrap_or_default();
    sanitize(&format!("{command}{args}"))
}

fn tool_state(result: Option<&Value>, view: &View) -> (String, Color) {
    let Some(result) = result else {
        return (
            if view.read_only || view.review.is_some() {
                "OUTCOME UNKNOWN".into()
            } else {
                "PENDING OBSERVATION".into()
            },
            AMBER,
        );
    };
    if result.get("error").is_some() {
        return ("ERROR / OUTCOME UNKNOWN".into(), AMBER);
    }
    let observed = result
        .get("result")
        .filter(|value| value.is_object())
        .unwrap_or(result);
    if observed.get("output_complete").and_then(Value::as_bool) == Some(false)
        || observed
            .get("output_errors")
            .and_then(Value::as_array)
            .is_some_and(|errors| !errors.is_empty())
    {
        let termination = observed.get("termination").and_then(Value::as_str);
        let status = match termination {
            Some(termination) if termination != "exited" => format!(
                "{} / CAPTURE INCOMPLETE",
                sanitize(&termination.to_uppercase())
            ),
            _ => "CAPTURE INCOMPLETE".into(),
        };
        return (status, AMBER);
    }
    if result.get("wait_timed_out").and_then(Value::as_bool) == Some(true) {
        return ("SESSION / WAIT TIMED OUT".into(), AMBER);
    }
    if let Some(termination) = observed.get("termination").and_then(Value::as_str)
        && termination != "exited"
    {
        return (sanitize(&termination.to_uppercase()), AMBER);
    }
    if let Some(code) = observed.get("exit_code").and_then(Value::as_i64) {
        if code != 0 {
            return (format!("EXIT {code}"), AMBER);
        }
        if observed.get("termination").and_then(Value::as_str) != Some("exited") {
            return ("EXIT 0 / TERMINATION UNKNOWN".into(), AMBER);
        }
        if ["stdout_truncated", "stderr_truncated"]
            .iter()
            .any(|field| observed.get(field).and_then(Value::as_bool) == Some(true))
        {
            return ("EXIT 0 / RETENTION TRUNCATED".into(), AMBER);
        }
        return if observed.get("output_complete").and_then(Value::as_bool) == Some(true) {
            ("EXIT 0 / CAPTURE COMPLETE".into(), CYAN)
        } else {
            ("EXIT 0 / CAPTURE UNKNOWN".into(), AMBER)
        };
    }
    if result.get("session_id").is_some() {
        let status = result
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or("observation");
        return (
            if status == "running" {
                "SESSION RUNNING".into()
            } else {
                format!("SESSION ACK / {}", sanitize(&status.to_uppercase()))
            },
            DIM,
        );
    }
    ("OUTCOME UNKNOWN".into(), AMBER)
}

fn append_streams(lines: &mut Vec<Line<'static>>, result: &Value) {
    let observed = result
        .get("result")
        .filter(|value| value.is_object())
        .unwrap_or(result);
    for (field, label, color) in [
        ("stdout_excerpt", "stdout", WHITE),
        ("stderr_excerpt", "stderr", AMBER),
    ] {
        let stream = result
            .get(field)
            .or_else(|| observed.get(field))
            .and_then(Value::as_str);
        if let Some(text) = stream.filter(|text| !text.is_empty()) {
            lines.push(Line::from(Span::styled(
                format!("  {label} / retained excerpt"),
                Style::default().fg(DIM),
            )));
            lines.extend(text_lines(&bounded(text, 2048), color));
        }
    }
    if result.get("error").is_some() {
        lines.extend(text_lines(
            &bounded(&result["error"].to_string(), 1024),
            AMBER,
        ));
    }
}

fn understanding(
    f: &mut Frame,
    area: Rect,
    state: &State,
    cards: &[ConversationCard],
    events: &[Event],
    view: &View,
    compact: bool,
) {
    let task_source = events.iter().rev().find(|event| matches!(&event.kind,
        Kind::Intent { revision, .. } | Kind::Correction { revision, .. } if *revision == state.revision))
        .map(|event| sanitize(&event.source.to_uppercase())).unwrap_or_else(|| "UNAVAILABLE".into());
    let interpretation_source = events
        .iter()
        .rev()
        .find(|event| {
            matches!(&event.kind,
        Kind::Interpretation { revision, .. } if *revision == state.interpretation_revision)
        })
        .map(|event| sanitize(&event.source.to_uppercase()))
        .unwrap_or_else(|| "UNAVAILABLE".into());
    let clean_interpretation = sanitize(&state.interpretation);
    let width = area.width.saturating_sub(2);
    let stale = !state.interpretation.is_empty() && state.interpretation_revision != state.revision;
    let mut lines = Vec::new();
    if state.unresolved {
        lines.push(Line::from(Span::styled(
            "CONFLICT / UNRESOLVED",
            Style::default().fg(AMBER).bold(),
        )));
    }
    if compact || area.height < 9 {
        lines.push(Line::from(Span::styled(
            fit_line(
                &format!(
                    "{interpretation_source} r{}{} / {}",
                    state.interpretation_revision,
                    if stale { " STALE" } else { "" },
                    if state.interpretation.is_empty() {
                        "not recorded"
                    } else {
                        &clean_interpretation
                    }
                ),
                width,
            ),
            Style::default().fg(if stale { AMBER } else { PINK }),
        )));
    } else {
        lines.push(Line::from(Span::styled(
            format!(
                "{interpretation_source} / INTERPRETATION r{}{}",
                state.interpretation_revision,
                if stale { " · STALE" } else { "" }
            ),
            Style::default()
                .fg(if stale { AMBER } else { PINK })
                .underlined(),
        )));
        lines.push(Line::from(Span::styled(
            fit_line(
                if state.interpretation.is_empty() {
                    "No interpretation recorded."
                } else {
                    &clean_interpretation
                },
                width,
            ),
            Style::default().fg(WHITE),
        )));
    }
    if let Some(card) = cards.iter().rev().find(|card| card.kind == CardKind::Tool) {
        let (status, color) = tool_state(card.result.as_ref(), view);
        lines.push(Line::from(Span::styled(
            format!("TOOL / {status}"),
            Style::default().fg(color),
        )));
    } else {
        lines.push(Line::from(Span::styled(
            "TOOL / no observation recorded",
            Style::default().fg(DIM),
        )));
    }
    if compact || area.height < 9 {
        lines.push(Line::from(Span::styled(
            fit_line(
                &format!("{task_source} r{} / {}", state.revision, state.goal),
                width,
            ),
            Style::default().fg(WHITE),
        )));
    } else {
        lines.push(Line::from(Span::styled(
            format!("{task_source} / TASK r{} · preview", state.revision),
            Style::default().fg(AMBER).underlined(),
        )));
        lines.push(Line::from(Span::styled(
            fit_line(&state.goal, width),
            Style::default().fg(WHITE),
        )));
    }
    lines.push(Line::from(Span::styled(
        fit_line(&format!("STATE / {}", state.status), width),
        Style::default().fg(DIM),
    )));
    if !compact {
        lines.push(Line::from(Span::styled(
            "Hidden provider reasoning: unavailable.",
            Style::default().fg(DIM),
        )));
    }
    f.render_widget(
        Paragraph::new(lines).block(panel(" UNDERSTANDING ", PINK)),
        area,
    );
}

fn correction_editor(f: &mut Frame, state: &State, view: &View, correction: &str) {
    let area = f.area().inner(Margin::new(3, 2));
    let block = panel(" CORRECT DIRECTION ", PINK);
    let inner = block.inner(area);
    f.render_widget(Clear, area);
    f.render_widget(block, area);
    let rows = Layout::vertical([
        Constraint::Length(2),
        Constraint::Length(2),
        Constraint::Length(2),
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .split(inner);
    let operation = OPERATIONS[view.operation];
    f.render_widget(
        Paragraph::new(vec![
            Line::from(Span::styled(
                fit_line(
                    &format!("OPERATION / {} · {}", operation.0, operation.1),
                    inner.width,
                ),
                Style::default().fg(PINK).bold(),
            )),
            Line::from(Span::styled(
                "Tab changes operation · Enter applies · Esc cancels",
                Style::default().fg(DIM),
            )),
        ]),
        rows[0],
    );
    let effect = if operation.1 == "decompression" {
        "Expand reopens up to 8 retained sources as marked context pins."
    } else {
        "The edited statement becomes a new intent revision; earlier evidence remains."
    };
    f.render_widget(
        Paragraph::new(sanitize(effect))
            .wrap(Wrap { trim: false })
            .style(Style::default().fg(DIM)),
        rows[1],
    );
    f.render_widget(
        Paragraph::new(vec![
            Line::from(fit_line(
                &format!("TASK r{} [preview] / {}", state.revision, state.goal),
                inner.width,
            )),
            Line::from(fit_line(
                &format!("CONSTRAINTS [preview] / {}", state.constraints.join("; ")),
                inner.width,
            )),
        ])
        .style(Style::default().fg(DIM)),
        rows[2],
    );
    f.render_widget(
        Paragraph::new("EDIT CORRECTION / viewport follows typed end")
            .style(Style::default().fg(CYAN)),
        rows[3],
    );
    let lines = wrap_lines(
        text_lines(&format!("{}▏", sanitize(correction)), WHITE),
        rows[4].width,
    );
    let start = lines.len().saturating_sub(usize::from(rows[4].height));
    f.render_widget(
        Paragraph::new(lines.into_iter().skip(start).collect::<Vec<_>>()),
        rows[4],
    );
    f.render_widget(
        Paragraph::new("No human acknowledgment or semantic agreement is inferred.")
            .style(Style::default().fg(DIM)),
        rows[5],
    );
}

fn composer(f: &mut Frame, area: Rect, state: &State, view: &View) {
    let reviewing = view.read_only || view.review.is_some();
    let title = if reviewing {
        " COMPOSER / READ-ONLY REVIEW "
    } else if view.composer.is_some() {
        " COMPOSER / Enter send · Esc browse "
    } else {
        " COMPOSER / i type · F4 Correct "
    };
    let block = panel(title, if reviewing { DIM } else { CYAN });
    let inner = block.inner(area);
    f.render_widget(block, area);
    let text = if reviewing {
        "History is read-only. F2 changes presentation; no actions rerun.".into()
    } else if let Some(text) = &view.composer {
        format!("› {}▏", sanitize(text))
    } else if !view.toast.is_empty() {
        sanitize(&view.toast)
    } else if !view.draft.is_empty() {
        format!("DRAFT / {}  · i to edit", sanitize(&view.draft))
    } else if !state.receipt.is_empty() {
        sanitize(&state.receipt)
    } else {
        format!(
            "{} · i type · F4 Correct direction",
            sanitize(&state.status)
        )
    };
    let lines = wrap_lines(text_lines(&text, WHITE), inner.width);
    let notice_height = usize::from(
        !view.toast.is_empty() && (view.composer.is_some() || reviewing) && inner.height > 1,
    );
    let start = lines
        .len()
        .saturating_sub(usize::from(inner.height).saturating_sub(notice_height));
    let mut visible = Vec::new();
    if notice_height > 0 {
        visible.extend(
            wrap_lines(text_lines(&format!("! {}", view.toast), AMBER), inner.width)
                .into_iter()
                .take(1),
        );
    }
    visible.extend(lines.into_iter().skip(start));
    f.render_widget(Paragraph::new(visible), inner);
}

fn footer(f: &mut Frame, area: Rect, width: u16, view: &View) {
    let lines = if width < 60 {
        vec!["F2 Expert · Ctrl-C stop", "i type · q quit · ? help"]
    } else if width < 100 {
        vec![
            "Enter send · F2 Expert · F3 Details",
            "F4 Correct · F5 Context · F6 Map",
            "PgUp/PgDn scroll · Esc browse · Ctrl-C stop",
            "i type · ↑/↓ select · Enter expand · q quit · ? help",
        ]
    } else {
        vec![
            "Enter send · F2 Expert · F3 Details · F4 Correct · F5 Context · F6 Map · PgUp/PgDn scroll",
            "Esc browse · i type · ↑/↓ select · Enter expand · Ctrl-C stop · q quit · ? help",
        ]
    };
    let color = if view.map_focus { CYAN } else { DIM };
    f.render_widget(
        Paragraph::new(lines.into_iter().map(Line::from).collect::<Vec<_>>())
            .style(Style::default().fg(color)),
        area,
    );
}

fn bounded(text: &str, limit: usize) -> String {
    let clean = sanitize(text);
    if clean.len() <= limit {
        clean
    } else {
        format!(
            "{}\n[display excerpt; F3 inspects retained content]",
            &clean[..clean.floor_char_boundary(limit)]
        )
    }
}

fn fit_line(text: &str, width: u16) -> String {
    let flat = sanitize(text).replace('\n', " ");
    if Span::raw(flat.as_str()).width() <= usize::from(width) {
        return flat;
    }
    let budget = usize::from(width.saturating_sub(1));
    let mut result = String::new();
    let mut used = 0;
    for character in flat.chars() {
        let cells = Span::raw(character.to_string()).width();
        if used + cells > budget {
            break;
        }
        result.push(character);
        used += cells;
    }
    if width > 0 {
        result.push('…');
    }
    result
}

fn text_lines(text: &str, color: Color) -> Vec<Line<'static>> {
    sanitize(text)
        .split('\n')
        .map(|line| {
            Line::from(Span::styled(
                format!("  {line}"),
                Style::default().fg(color),
            ))
        })
        .collect()
}

fn wrap_lines(lines: Vec<Line<'static>>, width: u16) -> Vec<Line<'static>> {
    if width == 0 {
        return Vec::new();
    }
    let mut wrapped = Vec::new();
    for line in lines {
        let mut spans = Vec::new();
        let mut used = 0;
        for span in line.spans {
            let mut fragment = String::new();
            for character in span.content.chars() {
                let cells = Span::raw(character.to_string()).width();
                if used + cells > usize::from(width) && used > 0 {
                    if !fragment.is_empty() {
                        spans.push(Span::styled(std::mem::take(&mut fragment), span.style));
                    }
                    wrapped.push(Line::from(std::mem::take(&mut spans)));
                    used = 0;
                }
                fragment.push(character);
                used += cells;
            }
            if !fragment.is_empty() {
                spans.push(Span::styled(fragment, span.style));
            }
        }
        wrapped.push(Line::from(spans));
    }
    wrapped
}

fn monochrome(f: &mut Frame, area: Rect, view: &View) {
    if !view.color {
        for y in area.top()..area.bottom() {
            for x in area.left()..area.right() {
                f.buffer_mut()[(x, y)]
                    .set_fg(Color::White)
                    .set_bg(Color::Black);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use ratatui::backend::TestBackend;
    use serde_json::json;

    fn event(id: &str, kind: Kind) -> Event {
        Event {
            schema: crate::model::SCHEMA,
            id: id.into(),
            run_id: "chat-render-test".into(),
            branch_id: "main".into(),
            seq: 1,
            timestamp: "2026-10-25T20:00:00Z".into(),
            source: match &kind {
                Kind::Intent { .. } | Kind::Correction { .. } => "user",
                Kind::ModelFinished { .. } | Kind::Interpretation { .. } => "model",
                Kind::ToolFinished { .. } => "executor",
                _ => "controller",
            }
            .into(),
            causes: vec![],
            kind,
        }
    }

    fn events() -> Vec<Event> {
        vec![
            event(
                "started",
                Kind::Started {
                    demo: false,
                    config: Config::default().inspector(),
                    source_revision: "test".into(),
                },
            ),
            event(
                "intent",
                Kind::Intent {
                    revision: 1,
                    statement: "qyr café 🦀 — run whoami".into(),
                    constraints: vec!["No shell".into()],
                },
            ),
        ]
    }

    fn tool_events(result: Option<Value>) -> Vec<Event> {
        let mut events = events();
        events.push(event("tool-start", Kind::ToolStarted { id:"tool-identity".into(), revision:1,
            invocation:json!({"tool":"exec","arguments":{"executable":"whoami","args":[],"cwd":"/workspace"}}) }));
        if let Some(result) = result {
            events.push(event(
                "tool-finish",
                Kind::ToolFinished {
                    id: "tool-identity".into(),
                    result,
                },
            ));
        }
        events
    }

    fn capture(events: &[Event], view: &View, width: u16, height: u16) -> String {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal
            .draw(|frame| render(frame, &State::at(events), events, view))
            .unwrap();
        terminal
            .backend()
            .buffer()
            .content
            .chunks(width as usize)
            .map(|row| row.iter().map(|cell| cell.symbol()).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn conversation_keeps_user_text_attributed_and_distinct_from_controls() {
        let mut view = View::new();
        view.composer = Some("qyr — my draft".into());
        let text = capture(&events(), &view, 140, 44);
        for expected in [
            "CHAT",
            "CONVERSATION",
            "USER",
            "qyr café",
            "qyr — my draft",
            "F2 Expert",
        ] {
            assert!(text.contains(expected), "missing {expected}:\n{text}");
        }
    }

    #[test]
    fn focused_composer_keeps_the_error_visible_beside_the_retained_draft() {
        let mut view = View::new();
        view.composer = Some("/tool {not-json café".into());
        view.toast = "tool must be valid JSON".into();
        let text = capture(&events(), &view, 140, 44);
        assert!(text.contains("/tool {not-json café"), "{text}");
        assert!(text.contains("tool must be valid JSON"), "{text}");
    }

    #[test]
    fn read_only_composer_shows_the_blocked_action_explanation() {
        let mut view = View::new();
        view.read_only = true;
        view.toast = "Read-only review; interruption blocked".into();
        let text = capture(&events(), &view, 140, 44);
        assert!(text.contains("READ-ONLY REVIEW"), "{text}");
        assert!(text.contains("interruption blocked"), "{text}");
    }

    #[test]
    fn tool_call_name_is_visible_when_the_invocation_uses_the_native_call_shape() {
        let mut events = events();
        events.push(event(
            "session-start",
            Kind::ToolStarted {
                id: "session-call".into(),
                revision: 1,
                invocation: json!({"name":"session","arguments":{"operation":"read","id":"s1"}}),
            },
        ));
        let text = capture(&events, &View::new(), 140, 44);
        assert!(text.contains("▣ session /"), "{text}");
    }

    #[test]
    fn wide_narrow_and_tiny_frames_keep_essential_navigation_and_policy() {
        for (width, height) in [(140, 44), (80, 30), (60, 24), (38, 14), (20, 8)] {
            let text = capture(&events(), &View::new(), width, height);
            assert!(text.contains("friendo"), "{width}x{height}:\n{text}");
            if width >= 60 {
                for expected in [
                    "CHAT",
                    "NORMAL",
                    "LIVE MAP",
                    "UNDERSTANDING",
                    "F2 Expert",
                    "Ctrl-C",
                    "q quit",
                ] {
                    assert!(
                        text.contains(expected),
                        "{width}x{height} missing {expected}:\n{text}"
                    );
                }
            }
        }
    }

    #[test]
    fn understanding_marks_a_prior_model_interpretation_stale() {
        let mut events = events();
        events.push(event(
            "interpretation",
            Kind::Interpretation {
                revision: 1,
                text: "Check the account".into(),
                basis: "model_statement".into(),
            },
        ));
        events.push(event(
            "correction",
            Kind::Correction {
                revision: 2,
                statement: "Only inspect this session".into(),
                operation: "disambiguation".into(),
                target: None,
            },
        ));
        let text = capture(&events, &View::new(), 140, 44);
        assert!(text.contains("UNDERSTANDING"));
        assert!(text.contains("STALE"), "{text}");
        assert!(text.contains("Check the account"), "{text}");
        assert!(text.contains("MODEL"));
    }

    #[test]
    fn understanding_preserves_fixture_attribution_instead_of_calling_it_model_reasoning() {
        let mut events = events();
        let mut interpretation = event(
            "fixture-interpretation",
            Kind::Interpretation {
                revision: 1,
                text: "Synthetic interpretation".into(),
                basis: "fixture".into(),
            },
        );
        interpretation.source = "demo_fixture".into();
        events.push(interpretation);
        let text = capture(&events, &View::new(), 140, 44);
        assert!(text.contains("DEMO_FIXTURE / INTERPRETATION"), "{text}");
    }

    #[test]
    fn long_task_and_interpretation_cannot_push_tool_observations_out_of_understanding() {
        let mut events = tool_events(Some(
            json!({"exit_code":2,"termination":"exited","output_complete":true}),
        ));
        events.push(event(
            "long-goal",
            Kind::Intent {
                revision: 2,
                statement: "A long task with multiple constraints. ".repeat(60),
                constraints: vec![],
            },
        ));
        events.push(event(
            "old-interpretation",
            Kind::Interpretation {
                revision: 1,
                text: "A long earlier interpretation. ".repeat(60),
                basis: "model_statement".into(),
            },
        ));
        let text = capture(&events, &View::new(), 140, 32);
        let side = text
            .lines()
            .skip(4)
            .map(|line| line.chars().skip(87).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(side.contains("STALE"), "{text}");
        assert!(side.contains("TOOL / EXIT 2"), "{text}");
    }

    #[test]
    fn correction_overlay_exposes_the_selected_expand_operation_and_its_effect() {
        let mut view = View::new();
        view.repair = Some("Retain café in the correction".into());
        view.operation = 1;
        let text = capture(&events(), &view, 140, 44);
        for expected in [
            "CORRECT DIRECTION",
            "Expand",
            "decompression",
            "Tab",
            "retained sources",
            "Retain café",
        ] {
            assert!(text.contains(expected), "missing {expected}:\n{text}");
        }
    }

    #[test]
    fn correction_editor_keeps_the_typed_tail_visible_after_long_context_and_input() {
        let mut events = events();
        events.push(event(
            "long-intent",
            Kind::Intent {
                revision: 2,
                statement: "A lengthy task context.\n".repeat(12),
                constraints: vec!["A retained explicit constraint. ".repeat(12)],
            },
        ));
        let mut view = View::new();
        view.repair = Some(format!(
            "{}EDITOR_END_MARKER",
            "Earlier correction line.\n".repeat(40)
        ));
        view.operation = 1;
        let text = capture(&events, &view, 80, 24);
        for expected in ["Expand", "decompression", "Tab", "EDITOR_END_MARKER"] {
            assert!(text.contains(expected), "missing {expected}:\n{text}");
        }
    }

    #[test]
    fn collapsed_chat_hides_raw_event_identifiers_to_leave_room_for_the_answer() {
        let mut events = events();
        events.push(event(
            "a628902f-97e9-4412-9b36-9439e381735e",
            Kind::ModelFinished {
                id: "request".into(),
                text: "The current account is alec.".into(),
                response_artifact: "artifacts/answer.txt".into(),
                usage: None,
            },
        ));
        let text = capture(&events, &View::new(), 60, 24);
        assert!(text.contains("The current account is alec."), "{text}");
        assert!(!text.contains("a628902f"), "{text}");
    }

    #[test]
    fn historical_missing_outcome_never_looks_like_a_completed_command() {
        let mut view = View::new();
        view.read_only = true;
        let events = tool_events(None);
        view.review = Some(events.len());
        let text = capture(&events, &view, 140, 44);
        assert!(text.contains("OUTCOME UNKNOWN"), "{text}");
        assert!(!text.contains("EXIT 0"), "{text}");
        assert!(text.contains("READ-ONLY"), "{text}");
    }

    #[test]
    fn tool_cards_distinguish_observed_exit_failure_capture_and_session_acknowledgment() {
        let cases = [
            (
                json!({"exit_code":0,"termination":"exited","output_complete":true,"stdout_excerpt":"alec\n"}),
                "EXIT 0 / CAPTURE COMPLETE",
            ),
            (
                json!({"exit_code":2,"termination":"exited","output_complete":true,"stderr_excerpt":"denied"}),
                "EXIT 2",
            ),
            (
                json!({"exit_code":0,"termination":"exited","output_complete":false}),
                "CAPTURE INCOMPLETE",
            ),
            (
                json!({"exit_code":null,"termination":"timeout","output_complete":true}),
                "TIMEOUT",
            ),
            (
                json!({"exit_code":null,"termination":"timeout","output_complete":false}),
                "TIMEOUT / CAPTURE INCOMPLETE",
            ),
            (
                json!({"session_id":"s1","status":"input written"}),
                "SESSION ACK",
            ),
            (
                json!({"session_id":"s1","status":"running","result":null}),
                "SESSION RUNNING",
            ),
            (
                json!({"error":"lost capture","outcome":"failure or unknown"}),
                "ERROR / OUTCOME UNKNOWN",
            ),
        ];
        for (result, expected) in cases {
            let text = capture(&tool_events(Some(result)), &View::new(), 140, 44);
            assert!(text.contains(expected), "missing {expected}:\n{text}");
            if expected != "EXIT 0 / CAPTURE COMPLETE" {
                assert!(!text.contains("CAPTURE COMPLETE"), "{text}");
            }
        }
    }

    #[test]
    fn expanded_tool_card_shows_observed_output_and_original_invocation() {
        let mut view = View::new();
        view.chat_selected = Some("tool-identity".into());
        view.chat_expanded.insert("tool-identity".into());
        let text = capture(
            &tool_events(Some(
                json!({"exit_code":2,"termination":"exited","output_complete":true,"stderr_excerpt":"denied"}),
            )),
            &view,
            140,
            50,
        );
        for expected in [
            "denied",
            "whoami",
            "/workspace",
            "tool-start",
            "tool-finish",
        ] {
            assert!(text.contains(expected), "missing {expected}:\n{text}");
        }
    }

    #[test]
    fn disposable_stream_preview_is_labeled_and_absent_from_historical_review() {
        let mut view = View::new();
        view.preview = "partial preview marker".into();
        let live = capture(&events(), &view, 140, 44);
        assert!(live.contains("STREAM PREVIEW / INCOMPLETE"), "{live}");
        assert!(live.contains("partial preview marker"));
        view.read_only = true;
        let historical = capture(&events(), &view, 140, 44);
        assert!(
            !historical.contains("partial preview marker"),
            "{historical}"
        );
    }

    #[test]
    fn no_color_applies_to_all_cells_including_tiny_frames() {
        for (width, height) in [(140, 44), (20, 8)] {
            let mut view = View::new();
            view.color = false;
            let events = events();
            let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
            terminal
                .draw(|frame| {
                    render(frame, &State::at(&events), &events, &view);
                    assert!(
                        frame
                            .buffer_mut()
                            .content
                            .iter()
                            .all(|cell| cell.fg == Color::White && cell.bg == Color::Black)
                    );
                })
                .unwrap();
        }
    }
}
