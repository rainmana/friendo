use crate::{
    controller::{Runtime, excerpt},
    model::{Event, Kind, Node, State},
    process::sanitize,
};
use anyhow::{Context, Result};
use crossterm::{
    event::{self, Event as Input, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    prelude::*,
    widgets::{
        Block, BorderType, Borders, Clear, List, ListItem, Paragraph, Wrap,
        canvas::{Canvas, Line as CanvasLine},
    },
};
use std::{
    collections::BTreeSet,
    io::{self, IsTerminal},
    path::PathBuf,
    time::{Duration, Instant},
};

const CYAN: Color = Color::Rgb(55, 235, 235);
const PINK: Color = Color::Rgb(249, 88, 198);
const AMBER: Color = Color::Rgb(249, 190, 72);
const INK: Color = Color::Rgb(5, 9, 17);
const PANEL: Color = Color::Rgb(10, 18, 29);
const DIM: Color = Color::Rgb(102, 133, 158);
const WHITE: Color = Color::Rgb(213, 231, 240);
const LENSES: [&str; 4] = ["runtime", "coordination", "evidence", "domain"];
const OPERATIONS: [(&str, &str); 5] = [
    ("Ground", "re-grounding"),
    ("Expand", "decompression"),
    ("Prioritize", "re-weighting"),
    ("Split", "disambiguation"),
    ("Sync", "synchronization"),
];

#[derive(Default)]
pub struct View {
    pub tab: usize,
    pub lens: usize,
    pub selected: Option<String>,
    pub list: bool,
    pub pan: (f64, f64),
    pub zoom: f64,
    pub filter: usize,
    pub edge_filter: usize,
    pub collapsed: bool,
    pub trace: u8,
    pub review: Option<usize>,
    pub inspect: bool,
    pub help: bool,
    pub composer: Option<String>,
    pub repair: Option<String>,
    pub operation: usize,
    pub toast: String,
    pub effects: String,
    pub color: bool,
    pub phase: f64,
    pub scroll: u16,
    pub context_content: String,
    pub retained: String,
}
impl View {
    pub fn new() -> Self {
        Self {
            zoom: 1.,
            effects: "full".into(),
            color: true,
            ..Self::default()
        }
    }
    fn nodes<'a>(&self, state: &'a State) -> Vec<&'a Node> {
        state
            .nodes
            .values()
            .filter(|n| n.lens == LENSES[self.lens])
            .filter(|n| !self.collapsed || !matches!(n.kind.as_str(), "request" | "process"))
            .filter(|n| {
                self.filter == 0
                    || match self.filter {
                        1 => n.stale,
                        2 => n.kind == "artifact",
                        3 => n.kind == "claim",
                        4 => n.synthetic,
                        _ => true,
                    }
            })
            .collect()
    }
    fn selection(&mut self, state: &State, step: isize) {
        let ids: Vec<_> = self.nodes(state).iter().map(|n| n.id.clone()).collect();
        if ids.is_empty() {
            self.selected = None;
            return;
        }
        let current = self
            .selected
            .as_ref()
            .and_then(|id| ids.iter().position(|x| x == id))
            .unwrap_or(0);
        let next = (current as isize + step).rem_euclid(ids.len() as isize) as usize;
        self.selected = Some(ids[next].clone());
    }
    fn selected<'a>(&self, state: &'a State) -> Option<&'a Node> {
        self.selected.as_ref().and_then(|id| state.nodes.get(id))
    }
}
struct TerminalGuard;
impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(
            io::stdout(),
            LeaveAlternateScreen,
            event::DisableBracketedPaste,
            crossterm::cursor::Show
        );
    }
}

pub async fn run(
    runtime: Option<Runtime>,
    mut events: Vec<Event>,
    dir: PathBuf,
    read_only: bool,
) -> Result<()> {
    if !io::stdout().is_terminal() || !io::stdin().is_terminal() {
        anyhow::bail!("TUI requires a terminal; use --headless for scripted operation");
    }
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = execute!(
            io::stdout(),
            LeaveAlternateScreen,
            event::DisableBracketedPaste,
            crossterm::cursor::Show
        );
        previous(info);
    }));
    enable_raw_mode()?;
    let _guard = TerminalGuard;
    execute!(
        io::stdout(),
        EnterAlternateScreen,
        event::EnableBracketedPaste
    )?;
    let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;
    let mut view = View::new();
    if let Some(r) = &runtime {
        if r.state().demo {
            view.lens = 1;
        }
        view.effects = r.config.effects.clone();
        view.color = r.config.color;
        view.list = r.config.layout == "list";
    }
    if read_only {
        view.review = Some(events.len());
        view.toast =
            "READ-ONLY REVIEW / [ ] move through retained events; tools cannot execute".into();
    }
    let mut seq = 0;
    let mut effect = Instant::now();
    let mut dirty = true;
    let mut last_preview = String::new();
    let mut state = State::default();
    let mut projected = usize::MAX;
    loop {
        if let Some(r) = &runtime {
            let store = r.store.lock().unwrap();
            if store.events.len() != events.len() {
                events = store.events.clone();
            }
        }
        let limit = view.review.unwrap_or(events.len()).min(events.len());
        if projected != limit {
            state = State::at(&events[..limit]);
            projected = limit;
        }
        if state.seq != seq {
            view.context_content = retained_file(&dir, &state.last_context)
                .unwrap_or_else(|e| format!("retained body unavailable: {e}"));
            seq = state.seq;
            effect = Instant::now();
            dirty = true;
            view.toast.clear();
        }
        if view.review.is_none()
            && let Some(r) = &runtime
        {
            let preview = r.preview.lock().unwrap().clone();
            if preview != last_preview {
                dirty = true;
                last_preview = preview.clone();
            }
            if !preview.is_empty() {
                state.status = format!("STREAMING / {}", excerpt(&preview, 500));
            }
        }
        let active = view.effects != "off"
            && effect.elapsed()
                < Duration::from_millis(match view.effects.as_str() {
                    "reduced" => 180,
                    "overdrive" => 1400,
                    _ => 950,
                });
        view.phase = if active {
            effect.elapsed().as_secs_f64() / if view.effects == "overdrive" { 1.5 } else { 1. }
        } else {
            2.
        };
        if view
            .selected
            .as_ref()
            .is_none_or(|id| !state.nodes.contains_key(id))
        {
            view.selection(&state, 0);
        }
        if dirty || active {
            terminal.draw(|f| render(f, &state, &events[..limit], &view))?;
            dirty = false;
        }
        if event::poll(Duration::from_millis(1))? {
            match event::read()? {
                Input::Key(key) if key.kind == KeyEventKind::Press => {
                    effect = Instant::now() - Duration::from_secs(2);
                    let mutable = !read_only && view.review.is_none();
                    match handle_key(
                        &mut view,
                        key,
                        &state,
                        &events,
                        runtime.as_ref(),
                        mutable,
                        &dir,
                    )
                    .await
                    {
                        Ok(true) => break,
                        Ok(false) => {}
                        Err(e) => view.toast = sanitize(&format!("{e:#}")),
                    }
                    if read_only && view.review.is_none() {
                        view.review = Some(events.len());
                    }
                    dirty = true;
                }
                Input::Paste(text) => {
                    if let Some(s) = view.composer.as_mut().or(view.repair.as_mut())
                        && s.len() + text.len() < 65536
                    {
                        s.push_str(&sanitize(&text));
                    }
                    dirty = true;
                }
                Input::Resize(..) => dirty = true,
                _ => {}
            }
        }
        tokio::time::sleep(Duration::from_millis(
            1000 / runtime.as_ref().map(|r| r.config.fps).unwrap_or(30),
        ))
        .await;
    }
    Ok(())
}
async fn handle_key(
    view: &mut View,
    key: KeyEvent,
    state: &State,
    events: &[Event],
    runtime: Option<&Runtime>,
    mutable: bool,
    dir: &std::path::Path,
) -> Result<bool> {
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
        if let Some(r) = runtime {
            let logged = r.cancel("Ctrl-C");
            r.interrupt_sessions().await?;
            logged?;
        }
        view.composer = None;
        view.repair = None;
        view.toast = "INTERRUPT REQUESTED / waiting for executor observation".into();
        return Ok(false);
    }
    if view.help || view.inspect {
        match key.code {
            KeyCode::Esc | KeyCode::Enter => {
                view.help = false;
                view.inspect = false;
                view.scroll = 0;
            }
            KeyCode::Down | KeyCode::PageDown => view.scroll = view.scroll.saturating_add(5),
            KeyCode::Up | KeyCode::PageUp => view.scroll = view.scroll.saturating_sub(5),
            _ => {}
        }
        return Ok(false);
    }
    if view.composer.is_some() || view.repair.is_some() {
        match key.code {
            KeyCode::Esc => {
                view.composer = None;
                view.repair = None;
            }
            KeyCode::Tab if view.repair.is_some() => view.operation = (view.operation + 1) % 5,
            KeyCode::Backspace => {
                if let Some(s) = view.composer.as_mut().or(view.repair.as_mut()) {
                    s.pop();
                }
            }
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                if let Some(s) = view.composer.as_mut().or(view.repair.as_mut())
                    && s.len() < 65536
                {
                    s.push(c);
                }
            }
            KeyCode::Enter => {
                let r = runtime.context("review has no controller")?;
                if let Some(text) = view.repair.take() {
                    let op = OPERATIONS[view.operation].1;
                    if op == "decompression" {
                        let ids: Vec<_> = state
                            .artifacts
                            .iter()
                            .map(|(k, v)| (k.clone(), v.clone()))
                            .collect();
                        let mut gaps = 0;
                        for (id, path) in ids.iter().take(8) {
                            let source = r.store.lock().unwrap().read_artifact(path);
                            match source {
                                Ok(s) => {
                                    r.event("user",Kind::Pin{node:id.clone(),text:excerpt(&s,16000),policy:"expanded retained source included in next request; bounded 16KiB excerpt explicitly marked".into()})?;
                                }
                                Err(_) => {
                                    gaps += 1;
                                }
                            }
                        }
                        if ids.is_empty() || gaps > 0 {
                            r.event("controller",Kind::Status{text:format!("SOURCE GAP / {} retained sources unavailable for expansion",gaps.max(1))})?;
                        }
                    }
                    if state.demo {
                        if !text.to_lowercase().contains("exported") {
                            anyhow::bail!(
                                "guided demo supports exported-symbol correction only; use run for arbitrary goals"
                            );
                        }
                        r.demo_repair(&text, op)?;
                    } else {
                        r.correct(&text, op, view.selected.clone())?;
                        if state.unresolved {
                            r.event(
                                "user",
                                Kind::Conflict {
                                    text: "Direct human Rewire correction supplied".into(),
                                    basis: "human_report".into(),
                                    unresolved: false,
                                },
                            )?;
                        }
                        if !r.busy() {
                            r.launch()?;
                        } else {
                            view.toast="Correction durable. Current turn stopping; submit /continue after it stops.".into();
                        }
                    }
                } else if let Some(text) = view.composer.take() {
                    if let Some(conflict) = text.strip_prefix("/conflict ") {
                        r.conflict(conflict)?;
                    } else if let Some(tool) = text.strip_prefix("/tool ") {
                        r.launch_tool(
                            serde_json::from_str(tool)
                                .context("tool must be {id,name,arguments} JSON")?,
                        )?;
                    } else if text == "/ack" {
                        r.acknowledge()?;
                    } else if let Some(items) = text.strip_prefix("/constraints ") {
                        r.constraints(
                            serde_json::from_str(items)
                                .context("constraints must be a JSON string array")?,
                        )?;
                    } else if let Some(report) = text.strip_prefix("/report ") {
                        let (mode, body) = report
                            .split_once(' ')
                            .context("report requires canonical_mode and report text")?;
                        let mode = serde_json::from_value(serde_json::json!(mode)).context(
                            "unknown canonical failure mode; use snake_case from glossary",
                        )?;
                        r.event(
                            "user",
                            Kind::Assessment {
                                mode,
                                basis: crate::model::AssessmentBasis::HumanReport,
                                possible: true,
                                revision: state.revision,
                                text: body.into(),
                            },
                        )?;
                    } else if text == "/continue" {
                        r.launch()?;
                    } else if !text.trim().is_empty() {
                        r.set_goal(&text, vec![])?;
                        if !r.busy() {
                            r.launch()?;
                        }
                    }
                }
            }
            _ => {}
        }
        return Ok(false);
    }
    let repair_key = runtime
        .and_then(|r| r.config.keys.get("repair"))
        .map(String::as_str)
        .unwrap_or("r");
    let yolo_key = runtime
        .and_then(|r| r.config.keys.get("yolo"))
        .map(String::as_str)
        .unwrap_or("y");
    let trace_key = runtime
        .and_then(|r| r.config.keys.get("trace"))
        .map(String::as_str)
        .unwrap_or("t");
    let pin_key = runtime
        .and_then(|r| r.config.keys.get("pin"))
        .map(String::as_str)
        .unwrap_or("p");
    if let KeyCode::Char(c) = key.code {
        let text = c.to_string();
        if text == repair_key {
            if mutable {
                view.repair = Some(state.goal.clone());
                view.operation = 3;
            } else {
                view.toast =
                    "Review is read-only. Press ] past the end to return to live mode.".into();
            }
            return Ok(false);
        }
        if text == yolo_key {
            if mutable {
                runtime.unwrap().autonomy()?;
            } else {
                view.toast = "Autonomy cannot change during review".into();
            }
            return Ok(false);
        }
        if text == trace_key {
            view.trace = if view.trace == 1 { 0 } else { 1 };
            return Ok(false);
        }
        if text == pin_key {
            if mutable && let Some(id) = &view.selected {
                runtime.unwrap().pin(id)?;
                view.toast = "PINNED / included in every subsequent outbound request".into();
            }
            return Ok(false);
        }
    }
    match key.code {
        KeyCode::Char('q') => return Ok(true),
        KeyCode::Esc => {
            view.review = None;
            view.trace = 0;
        }
        KeyCode::Char('?') => view.help = true,
        KeyCode::Char('1'..='5') => {
            if let KeyCode::Char(c) = key.code {
                view.tab = c.to_digit(10).unwrap() as usize - 1;
                view.scroll = 0;
            }
        }
        KeyCode::Tab => {
            view.lens = (view.lens + 1) % 4;
            view.selection(state, 0);
        }
        KeyCode::Char('j') => view.selection(state, 1),
        KeyCode::Char('k') => view.selection(state, -1),
        KeyCode::Char('v') => view.list = !view.list,
        KeyCode::Char('c') => view.collapsed = !view.collapsed,
        KeyCode::Char('/') => {
            view.filter = (view.filter + 1) % 5;
            view.selection(state, 0);
        }
        KeyCode::Char('.') => view.edge_filter = (view.edge_filter + 1) % 4,
        KeyCode::Char('d') => view.trace = if view.trace == 2 { 0 } else { 2 },
        KeyCode::Char('+') | KeyCode::Char('=') => view.zoom = (view.zoom * 1.2).min(3.),
        KeyCode::Char('-') => view.zoom = (view.zoom / 1.2).max(0.4),
        KeyCode::Left => view.pan.0 -= 12.,
        KeyCode::Right => view.pan.0 += 12.,
        KeyCode::Up => view.pan.1 += 12.,
        KeyCode::Down => view.pan.1 -= 12.,
        KeyCode::Char('f') => {
            if let Some(n) = view.selected(state) {
                view.pan = position(n, events);
            }
        }
        KeyCode::Char('a') => {
            if let Some(n) = events.iter().rev().find_map(|e| {
                if let Kind::Node { node } = &e.kind {
                    if node.kind == "process" || node.kind == "request" {
                        state.nodes.get(&node.id)
                    } else {
                        None
                    }
                } else {
                    None
                }
            }) {
                view.selected = Some(n.id.clone());
                view.lens = LENSES.iter().position(|l| *l == n.lens).unwrap_or(0);
                view.pan = position(n, events);
            }
        }
        KeyCode::Char('0') => {
            view.pan = (0., 0.);
            view.zoom = 1.;
        }
        KeyCode::Char('[') => {
            view.review = Some(view.review.unwrap_or(events.len()).saturating_sub(1))
        }
        KeyCode::Char(']') => {
            let n = view.review.unwrap_or(events.len()).saturating_add(1);
            view.review = if n > events.len() { None } else { Some(n) };
        }
        KeyCode::Char('i') if mutable => view.composer = Some(String::new()),
        KeyCode::Char(' ') if mutable && state.demo => {
            let r = runtime.unwrap();
            if !state.nodes.contains_key("answer") {
                r.demo_reveal()?;
            } else if state.revision == 1 {
                r.demo_correct()?;
                view.lens = 1;
                view.selected = Some("interpretation".into());
            } else {
                view.repair = Some(state.goal.clone());
                view.operation = 3;
            }
        }
        KeyCode::Enter => {
            view.retained = if view.tab == 3 {
                view.context_content.clone()
            } else if let Some(node) = view.selected(state) {
                if node.kind == "artifact" {
                    retained_file(dir, &node.detail).unwrap_or_else(|e| format!("SOURCE GAP: {e}"))
                } else {
                    String::new()
                }
            } else {
                String::new()
            };
            view.inspect = true;
            view.scroll = 0;
        }
        KeyCode::PageDown => view.scroll = view.scroll.saturating_add(8),
        KeyCode::PageUp => view.scroll = view.scroll.saturating_sub(8),
        _ => {}
    }
    Ok(false)
}
fn panel(title: impl Into<Line<'static>>, color: Color) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(color))
        .title(title)
        .style(Style::default().bg(PANEL).fg(WHITE))
}
fn retained_file(dir: &std::path::Path, reference: &str) -> Result<String> {
    if reference.is_empty() {
        anyhow::bail!("no request delivered yet");
    }
    let path = dir.join(reference).canonicalize()?;
    if !path.starts_with(dir.canonicalize()?) || std::fs::metadata(&path)?.len() > 2 * 1024 * 1024 {
        anyhow::bail!(
            "artifact outside run directory or exceeds 2MiB inline view; inspect retained file directly"
        );
    }
    Ok(sanitize(&String::from_utf8_lossy(&std::fs::read(path)?)))
}
fn position(node: &Node, events: &[Event]) -> (f64, f64) {
    let group: Vec<_> = events
        .iter()
        .filter_map(|e| {
            if let Kind::Node { node: n } = &e.kind {
                if n.lens == node.lens {
                    Some(n.id.as_str())
                } else {
                    None
                }
            } else {
                None
            }
        })
        .collect();
    let mut seen = BTreeSet::new();
    let ids: Vec<_> = group.into_iter().filter(|id| seen.insert(*id)).collect();
    let i = ids.iter().position(|id| *id == node.id).unwrap_or(0);
    ((i % 3) as f64 * 62. - 62., 24. - (i / 3) as f64 * 27.)
}
pub fn render(f: &mut Frame, state: &State, events: &[Event], view: &View) {
    let area = f.area();
    f.render_widget(
        Block::default().style(Style::default().bg(INK).fg(WHITE)),
        area,
    );
    if area.width < 38 || area.height < 14 {
        f.render_widget(Paragraph::new("friendo / terminal too small\nResize to at least 38 × 14\nq quit • Ctrl-C interrupt").style(Style::default().fg(CYAN)),area);
        return;
    }
    let rows = Layout::vertical([
        Constraint::Length(4),
        Constraint::Length(1),
        Constraint::Min(4),
        Constraint::Length(if area.height < 26 { 3 } else { 6 }),
        Constraint::Length(3),
        Constraint::Length(1),
    ])
    .split(area);
    let mode = if state.autonomy == "yolo" {
        "YOLO"
    } else {
        "NORMAL"
    };
    let review = view
        .review
        .map(|n| format!("REVIEW {n}/{}", events.len()))
        .unwrap_or_else(|| "LIVE".into());
    let title = Line::from(vec![
        Span::styled(" ◇ friendo ", Style::default().fg(INK).bg(CYAN).bold()),
        Span::styled(" / CONTAINMENT IS A METAPHOR ", Style::default().fg(DIM)),
        Span::styled(
            if state.demo {
                " OFFLINE DEMO / SYNTHETIC "
            } else {
                " NATIVE AGENT CONSOLE "
            },
            Style::default().fg(AMBER),
        ),
    ]);
    let status = Line::from(vec![
        Span::styled(
            format!(" AUTONOMY: {mode} "),
            Style::default()
                .fg(if mode == "YOLO" { PINK } else { CYAN })
                .bold(),
        ),
        Span::raw(format!(
            "│ INTENT r{} │ {} │ FX {} │ ",
            state.revision, review, view.effects
        )),
        Span::styled("COST ?", Style::default().fg(DIM)),
    ]);
    let config = events.iter().find_map(|e| {
        if let Kind::Started { config, .. } = &e.kind {
            Some(config)
        } else {
            None
        }
    });
    let alias = config
        .and_then(|c| c.pointer("/fields/active_provider/value"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or("none");
    let provider = config
        .and_then(|c| c.pointer("/fields/providers/value"))
        .and_then(|v| v.get(alias));
    let model = provider
        .and_then(|p| p.get("model"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or(if state.demo {
            "deterministic fixture"
        } else {
            "unconfigured"
        });
    let policy = config
        .and_then(|c| c.pointer("/fields/execution/value/allowed_executables"))
        .and_then(serde_json::Value::as_array)
        .map(Vec::len)
        .unwrap_or(0);
    let transport = Line::from(Span::styled(
        sanitize(&format!(
            " PROVIDER {alias} / {model}  | EXEC {policy} admitted | SHELL {}",
            if config
                .and_then(|c| c.pointer("/fields/execution/value/allow_shell"))
                .and_then(serde_json::Value::as_bool)
                == Some(true)
            {
                "ON"
            } else {
                "OFF"
            }
        )),
        Style::default().fg(DIM),
    ));
    f.render_widget(
        Paragraph::new(Text::from(vec![status, transport]))
            .block(panel(title, if state.unresolved { AMBER } else { CYAN })),
        rows[0],
    );
    let labels = [
        "1 TOPOLOGY",
        "2 INTENT",
        "3 EVIDENCE",
        "4 CONTEXT",
        "5 CONFIG",
    ];
    f.render_widget(
        Paragraph::new(Line::from(
            labels
                .iter()
                .enumerate()
                .map(|(i, t)| {
                    Span::styled(
                        format!(" {t} "),
                        if i == view.tab {
                            Style::default().fg(INK).bg(PINK).bold()
                        } else {
                            Style::default().fg(DIM)
                        },
                    )
                })
                .collect::<Vec<_>>(),
        )),
        rows[1],
    );
    let keys = config.and_then(|c| c.pointer("/fields/keys/value"));
    let binding = |name: &str, default: &'static str| {
        keys.and_then(|k| k.get(name))
            .and_then(serde_json::Value::as_str)
            .unwrap_or(default)
    };
    let repair_key = binding("repair", "r");
    let yolo_key = binding("yolo", "y");
    let trace_key = binding("trace", "t");
    let content = rows[2];
    match view.tab {
        0 => render_map(f, content, state, events, view),
        1 => {
            let text = format!(
                "EXACT USER STATEMENT / r{}\n{}\n\nEXPLICIT CONSTRAINTS (retained across corrections)\n{}\n\nRECORDED INTERPRETATION / r{}\n{}\n\nDELIVERY / r{} • HUMAN ACKNOWLEDGMENT / not inferred\n\nPINS / next outbound request\n{}\n\nRECEIPT\n{}",
                state.revision,
                state.goal,
                state
                    .constraints
                    .iter()
                    .map(|s| format!("◆ {s}"))
                    .collect::<Vec<_>>()
                    .join("\n"),
                state.interpretation_revision,
                state.interpretation,
                state.delivered_revision,
                state
                    .pins
                    .iter()
                    .map(|(k, v)| format!("{k}: {v}"))
                    .collect::<Vec<_>>()
                    .join("\n"),
                state.receipt
            );
            let text = format!(
                "{text}\n\nEXPLICIT ACKNOWLEDGMENT / revision {:?}\nPOSSIBLE FAILURE REPORTS (attributed)\n{}",
                state.acknowledged_revision,
                serde_json::to_string_pretty(&state.assessments).unwrap_or_default()
            );
            f.render_widget(
                Paragraph::new(sanitize(&text))
                    .wrap(Wrap { trim: false })
                    .scroll((view.scroll, 0))
                    .block(panel(
                        " INTENT / INTERPRETATION / application records ",
                        PINK,
                    )),
                content,
            );
        }
        2 => {
            let text = format!(
                "ANSWER / retained model output\n{}\n\nARTIFACTS / exact source references\n{}\n\nDECLARED CHECKS\n{}\n\nRUNNING (review without completion means outcome unknown)\n{}",
                state.transcript,
                state
                    .artifacts
                    .iter()
                    .map(|(k, v)| format!("{k}\n  {v}"))
                    .collect::<Vec<_>>()
                    .join("\n"),
                state
                    .checks
                    .iter()
                    .map(|(k, v)| format!("{} {k}", if *v { "PASS" } else { "PENDING/FAIL" }))
                    .collect::<Vec<_>>()
                    .join("\n"),
                serde_json::to_string_pretty(&state.running).unwrap_or_default()
            );
            f.render_widget(
                Paragraph::new(sanitize(&text))
                    .wrap(Wrap { trim: false })
                    .scroll((view.scroll, 0))
                    .block(panel(" EXECUTION / EVIDENCE / artifacts retained ", CYAN)),
                content,
            );
        }
        3 => {
            let text = format!(
                "ACTUAL OUTBOUND BODY\n{}\n\nThe request artifact includes selected user intent, explicit constraints, pinned source content, tool schemas, exact model-visible excerpts, and provider-specific transformations.\n\nProvider-internal context: UNAVAILABLE\nCredential headers: excluded\n\nUse friendo replay <id> --context to print the exact request body.\n\nPINS / inclusion policy: every subsequent request, credential redaction only\n{}",
                state.last_context,
                state
                    .pins
                    .iter()
                    .map(|(k, v)| format!("{k}\n{v}"))
                    .collect::<Vec<_>>()
                    .join("\n")
            );
            f.render_widget(
                Paragraph::new(sanitize(&format!(
                    "{text}\n\nRETAINED REQUEST BODY\n{}",
                    view.context_content
                )))
                .wrap(Wrap { trim: false })
                .scroll((view.scroll, 0))
                .block(panel(" CONTEXT / retained outbound content ", PINK)),
                content,
            );
        }
        _ => {
            let mut config = events
                .iter()
                .find_map(|e| {
                    if let Kind::Started { config, .. } = &e.kind {
                        Some(config)
                    } else {
                        None
                    }
                })
                .cloned();
            if let Some(fields) = config
                .as_mut()
                .and_then(|c| c.get_mut("fields"))
                .and_then(serde_json::Value::as_object_mut)
            {
                let authority = events.iter().rev().find(|e| {
                    matches!(e.kind, Kind::Autonomy { .. })
                        && matches!(e.source.as_str(), "user" | "user_config")
                });
                fields.insert("autonomy".into(),serde_json::json!({"value":state.autonomy,"source":authority.map(|e|serde_json::json!({"actor":e.source,"event":e.id})),"applies":"next action dispatch; effective session selection","precedence":"direct user selection after startup config"}));
            }
            let text = config
                .map(|c| serde_json::to_string_pretty(&c).unwrap_or_default())
                .unwrap_or_default();
            f.render_widget(
                Paragraph::new(sanitize(&text))
                    .wrap(Wrap { trim: false })
                    .scroll((view.scroll, 0))
                    .block(panel(" EFFECTIVE CONFIGURATION / source + boundary ", DIM)),
                content,
            );
        }
    }
    let timeline: Vec<Line> = events
        .iter()
        .rev()
        .take(rows[3].height.saturating_sub(2) as usize)
        .map(|e| {
            let text = event_label(e);
            Line::from(vec![
                Span::styled(format!(" {:04} ", e.seq), Style::default().fg(DIM)),
                Span::styled(
                    text,
                    Style::default().fg(match e.kind {
                        Kind::Correction { .. } | Kind::Failure { .. } => AMBER,
                        Kind::Restored { .. } | Kind::Check { passed: true, .. } => CYAN,
                        Kind::Interpretation { .. } => PINK,
                        _ => WHITE,
                    }),
                ),
            ])
        })
        .collect();
    f.render_widget(
        Paragraph::new(timeline).block(panel(" EVENT TRACE / append-only ", DIM)),
        rows[3],
    );
    let prompt = if let Some(s) = &view.composer {
        format!("> {s}▏")
    } else if let Some(s) = &view.repair {
        format!("REWIRE > {s}▏")
    } else if !view.toast.is_empty() {
        view.toast.clone()
    } else if !state.receipt.is_empty() {
        state.receipt.clone()
    } else {
        state.status.clone()
    };
    f.render_widget(
        Paragraph::new(sanitize(&prompt)).block(panel(
            if view.composer.is_some() {
                " COMPOSER / Enter submit · Esc discard ".into()
            } else {
                format!(" COMMAND BUS / i compose · {repair_key} Rewire · Ctrl-C interrupt ")
            },
            if state.restored { CYAN } else { AMBER },
        )),
        rows[4],
    );
    f.render_widget(Paragraph::new(if area.width<90{"? help  q quit  Tab lens  j/k select  Enter inspect".to_owned()}else{format!("? help  q quit  i compose  {repair_key} Rewire  {yolo_key} YOLO  Tab lens  j/k select  {trace_key} trace  d impact  [ ] history  Space demo")}).style(Style::default().fg(DIM)),rows[5]);
    if view.help {
        overlay(
            f,
            " FIELD MANUAL / Esc close ",
            &format!(
                "ACTIVE REMAPS: repair={repair_key}, yolo={yolo_key}, trace={trace_key}, pin={}\nExamples below use default bindings.\n\n{HELP}",
                binding("pin", "p")
            ),
            view.scroll,
            CYAN,
        );
    }
    if view.inspect {
        let text = if !view.retained.is_empty() {
            view.retained.clone()
        } else if state.restored && view.tab == 1 {
            events
                .iter()
                .rev()
                .find_map(|e| {
                    if let Kind::Restored { .. } = e.kind {
                        serde_json::to_string_pretty(e).ok()
                    } else {
                        None
                    }
                })
                .unwrap_or_default()
        } else if let Some(n) = view.selected(state) {
            let edges: Vec<_> = state
                .edges
                .iter()
                .filter(|e| e.from == n.id || e.to == n.id)
                .collect();
            serde_json::to_string_pretty(&serde_json::json!({"node":n,"relationships":edges,"receipt_conditions":events.iter().rev().find(|e|matches!(e.kind,Kind::Restored{..}))})).unwrap_or_default()
        } else {
            "No selected node; Tab lens and j/k selection".into()
        };
        overlay(
            f,
            " PROVENANCE / exact recorded entities + source events ",
            &sanitize(&text),
            view.scroll,
            CYAN,
        );
    }
    if let Some(text) = &view.repair {
        let selected = view.selected(state);
        let body = format!(
            "{}\nCANONICAL OPERATION: {} (application mapping)\nTab selects: Ground / Expand / Prioritize / Split / Sync\n\nEXACT INSTRUCTION / direct user / r{}\n{}\nCONSTRAINTS: {}\n\nCURRENT INTERPRETATION / r{}\n{}\n\nSELECTED DEPENDENCY\n{}\nKNOWN AFFECTED WORK: {} nodes (impact coverage is not exhaustive)\n\nEDIT CORRECTION (Backspace edits, paste supported)\n{}▏\n\nEnter applies immediately. Esc cancels. Effects never delay execution.\nPrioritize: enter explicit ordering in correction. Expand: up to 8 retained sources, marked 16KiB excerpts.\nNo human acknowledgment is inferred.",
            OPERATIONS[view.operation].0,
            OPERATIONS[view.operation].1,
            state.revision,
            state.goal,
            state.constraints.join("; "),
            state.interpretation_revision,
            state.interpretation,
            selected
                .map(|n| format!("{} / stale {} / source {}", n.label, n.stale, n.event))
                .unwrap_or_else(|| "none".into()),
            state.nodes.values().filter(|n| n.stale).count(),
            text
        );
        overlay(
            f,
            " ⌁ REWIRE / trace · revise · deliver · check ",
            &sanitize(&body),
            0,
            PINK,
        );
    }
    if !view.color {
        let buffer = f.buffer_mut();
        for y in area.top()..area.bottom() {
            for x in area.left()..area.right() {
                buffer[(x, y)].set_fg(Color::White).set_bg(Color::Black);
            }
        }
    }
}
fn render_map(f: &mut Frame, area: Rect, state: &State, events: &[Event], view: &View) {
    let wide = area.width >= 100;
    let parts = Layout::horizontal([
        Constraint::Percentage(if wide { 70 } else { 100 }),
        Constraint::Percentage(if wide { 30 } else { 0 }),
    ])
    .split(area);
    let title = format!(
        " {} / {} / {} nodes / {} ",
        LENSES[view.lens].to_uppercase(),
        if view.list || area.width < 90 {
            "LIST"
        } else {
            "TOPOLOGY"
        },
        view.nodes(state).len(),
        ["all", "stale", "artifacts", "claims", "synthetic"][view.filter]
    );
    if view.list || area.width < 90 {
        let list = List::new(
            view.nodes(state)
                .iter()
                .map(|n| {
                    ListItem::new(format!(
                        "{} {} {} {}",
                        if view.selected.as_deref() == Some(&n.id) {
                            "▶"
                        } else {
                            " "
                        },
                        if n.stale { "[STALE]" } else { "[●]" },
                        n.label,
                        if n.synthetic { "[SYNTHETIC]" } else { "" }
                    ))
                    .style(Style::default().fg(if n.stale {
                        AMBER
                    } else {
                        CYAN
                    }))
                })
                .collect::<Vec<_>>(),
        )
        .block(panel(title, CYAN));
        f.render_widget(list, parts[0]);
    } else {
        let nodes = view.nodes(state);
        let ids: BTreeSet<_> = nodes.iter().map(|n| n.id.as_str()).collect();
        let traced = if view.trace > 0 {
            view.selected
                .as_ref()
                .map(|id| state.trace(id, view.trace == 2))
                .unwrap_or_default()
        } else {
            vec![]
        };
        let x = 110. / view.zoom;
        let y = 50. / view.zoom;
        let canvas = Canvas::default()
            .block(panel(title, CYAN))
            .marker(symbols::Marker::Braille)
            .x_bounds([view.pan.0 - x, view.pan.0 + x])
            .y_bounds([view.pan.1 - y, view.pan.1 + y])
            .paint(|ctx| {
                for e in &state.edges {
                    if !ids.contains(e.from.as_str()) || !ids.contains(e.to.as_str()) {
                        continue;
                    }
                    if view.edge_filter > 0
                        && !match view.edge_filter {
                            1 => e.relation == "depends_on",
                            2 => e.relation == "supports",
                            3 => e.relation == "produced",
                            _ => true,
                        }
                    {
                        continue;
                    }
                    let a = &state.nodes[&e.from];
                    let b = &state.nodes[&e.to];
                    let (x1, y1) = position(a, events);
                    let (x2, y2) = position(b, events);
                    let stale = a.stale || b.stale;
                    let color = if stale {
                        AMBER
                    } else if traced.contains(&a.id) && traced.contains(&b.id) {
                        Color::White
                    } else {
                        Color::Rgb(38, 95, 116)
                    };
                    if stale {
                        for seg in 0..7 {
                            let t = seg as f64 / 7.;
                            let u = (seg as f64 + 0.5) / 7.;
                            ctx.draw(&CanvasLine {
                                x1: x1 + (x2 - x1) * t,
                                y1: y1 + (y2 - y1) * t,
                                x2: x1 + (x2 - x1) * u,
                                y2: y1 + (y2 - y1) * u,
                                color,
                            });
                        }
                    } else {
                        ctx.draw(&CanvasLine {
                            x1,
                            y1,
                            x2,
                            y2,
                            color,
                        });
                    }
                    if view.phase < 0.95 && view.effects != "reduced" {
                        let t = view.phase / 0.95;
                        let px = x1 + (x2 - x1) * t;
                        let py = y1 + (y2 - y1) * t;
                        ctx.print(px, py, Span::styled("◆", Style::default().fg(CYAN)));
                    }
                }
                for n in &nodes {
                    let (px, py) = position(n, events);
                    let selected = view.selected.as_deref() == Some(&n.id);
                    let color = if n.stale {
                        AMBER
                    } else if n.lens == "coordination" {
                        PINK
                    } else {
                        CYAN
                    };
                    let icon = match n.kind.as_str() {
                        "agent" => "◇",
                        "artifact" => "▤",
                        "claim" => "◆",
                        "intent" => "⌁",
                        "process" | "session" => "▣",
                        "host" => "◈",
                        _ => "○",
                    };
                    ctx.print(
                        px - 8.,
                        py,
                        Span::styled(
                            format!(
                                "{} {icon} {}",
                                if selected { "▶" } else { " " },
                                short_label(&sanitize(&n.label))
                            ),
                            Style::default()
                                .fg(if selected { WHITE } else { color })
                                .add_modifier(if selected {
                                    Modifier::BOLD
                                } else {
                                    Modifier::empty()
                                }),
                        ),
                    );
                    ctx.print(
                        px - 5.,
                        py - 5.,
                        Span::styled(
                            format!(
                                "{}{}",
                                if n.stale { "STALE / " } else { "" },
                                if n.synthetic { "SYNTHETIC" } else { &n.kind }
                            ),
                            Style::default().fg(DIM),
                        ),
                    );
                }
            });
        f.render_widget(canvas, parts[0]);
    }
    if wide {
        let selected = view.selected(state);
        let content=selected.map(|n|format!("{}\n{}\n{}\n\n{}\n\nREVISION: {:?}\nSTALE: {}\nSOURCE EVENT\n{}\n\nRELATIONSHIPS\n{}",n.label,n.kind,if n.synthetic{"SYNTHETIC FIXTURE"}else{"RECORDED ENTITY"},n.detail,n.revision,n.stale,n.event,state.edges.iter().filter(|e|e.from==n.id||e.to==n.id).map(|e|format!("{} → {}\n  {} / {}",e.from,e.to,e.relation,e.basis)).collect::<Vec<_>>().join("\n"))).unwrap_or_else(||"No entities in this lens\nTab changes lens\n/ changes filter".into());
        f.render_widget(
            Paragraph::new(sanitize(&content))
                .wrap(Wrap { trim: false })
                .block(panel(" SELECTED / Enter full provenance ", PINK)),
            parts[1],
        );
    }
}
fn short_label(text: &str) -> String {
    let mut s = text.chars().take(22).collect::<String>();
    if text.chars().count() > 22 {
        s.push('…');
    }
    s
}
fn overlay(f: &mut Frame, title: &str, text: &str, scroll: u16, color: Color) {
    let a = f.area();
    let width = a.width.saturating_sub(6).min(112);
    let height = a.height.saturating_sub(4);
    let rect = Rect::new(
        (a.width - width) / 2,
        (a.height - height) / 2,
        width,
        height,
    );
    f.render_widget(Clear, rect);
    f.render_widget(
        Paragraph::new(text.to_owned())
            .wrap(Wrap { trim: false })
            .scroll((scroll, 0))
            .block(panel(title.to_owned(), color)),
        rect,
    );
}
fn event_label(e: &Event) -> String {
    match &e.kind {
        Kind::Correction {
            revision,
            operation,
            ..
        } => format!("REVISION r{revision} / {operation} / dependent work stale"),
        Kind::Delivered { revision, .. } => format!("CONTEXT DELIVERED r{revision}"),
        Kind::Interpretation { revision, .. } => {
            format!("INTERPRETATION UPDATED r{revision} / no human acknowledgment inferred")
        }
        Kind::Check {
            passed, condition, ..
        } => format!(
            "{} / {}",
            if *passed {
                "CHECK PASSED"
            } else {
                "CHECK PENDING/FAILED"
            },
            condition
        ),
        Kind::Restored { .. } => "LINK RESTORED / Enter in Intent shows conditions".into(),
        Kind::Status { text } => excerpt(&sanitize(text), 180),
        Kind::ToolStarted { id, .. } => format!("EXECUTION DISPATCH / {id}"),
        Kind::ToolFinished { id, .. } => format!("EXECUTION OBSERVED / {id}"),
        Kind::Autonomy { mode, .. } => {
            format!("AUTONOMY: {} / direct user authority", mode.to_uppercase())
        }
        Kind::Failure { text, outcome } => format!("{outcome} / {text}"),
        _ => format!(
            "{} / {}",
            e.source,
            serde_json::to_value(&e.kind)
                .ok()
                .and_then(|v| v.get("type").cloned())
                .unwrap_or_default()
        ),
    }
}
const HELP: &str = "NAVIGATION\n1 topology · 2 intent · 3 evidence · 4 context · 5 config\nTab lens: Runtime / Coordination / Evidence / Domain (synthetic fixture)\nj/k select · Enter inspector · v list alternative · c collapse process/request groups\nArrow keys pan · +/- zoom · f focus selected · a focus latest work · 0 reset camera\n/ node filter: all/stale/artifact/claim/synthetic · . edge filter: all/depends_on/supports/produced\nt upstream trace · d downstream impact (recorded edges only)\n[ previous event · ] next event (past end returns live) · Esc return live\nPageUp/PageDown scroll main inspectors · arrows scroll overlay\n\nOPERATION\ni command composer · Enter submit · /continue resumes using current context\n/conflict <text> records an unresolved human-reported interpretation conflict\n/tool JSON submits a direct tool call {id,name,arguments} through the same execution policy\n/ack explicitly acknowledges current interpretation · /constraints [JSON_STRING_ARRAY] replaces constraints\n/report canonical_mode text records a possible failure with human-report provenance\nr Rewire · Tab picks Ground/Expand/Prioritize/Split/Sync · edit correction · Enter applies\np pins selected source detail (next outbound requests; inspector shows inclusion)\ny toggles NORMAL/YOLO (only direct user or config can change autonomy)\nSpace advances credential-free guided demo; r applies exported-only repair\nCtrl-C interrupts the active turn and retained sessions; q quits and terminates retained sessions\n\nPOLICY AND EVIDENCE\nYOLO permits bounded assumptions under the same executor policy; explicit constraints stay.\nNormal pauses dependent work for explicit unresolved conflicts. Semantic conflict detection is unavailable.\nReview is read-only. Returning live never reruns an action automatically.\nRewire receipts concern recorded delivery, interpretation, and declared checks. MCW is emergent, not stored.\nProvider-internal context and unreported pricing are unknown.\nPTY terminal emulation, network discovery, resume/rerun/fork are unavailable in this milestone.\n\nCONFIGURATION\nfriendo.toml configures provider aliases, models, endpoints, system prompt, tool admission/limits,\nvisual effects off/reduced/full/overdrive, color, FPS, map/list layout, and r/y/t/p remaps.\nFile edits apply on next run; command line overrides file values. Runtime y applies at next dispatch.\nNo tool output or imported source can mutate user configuration.";

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use ratatui::backend::TestBackend;
    #[tokio::test]
    async fn historical_controls_never_dispatch_or_change_user_state() {
        let root = tempfile::tempdir().unwrap();
        let r = Runtime::new(root.path(), Config::default(), true).unwrap();
        r.seed_demo().unwrap();
        let events = r.store.lock().unwrap().events.clone();
        let state = r.state();
        let dir = r.store.lock().unwrap().dir.clone();
        let mut view = View::new();
        view.review = Some(events.len());
        for c in ['r', 'y', 'p', 'i', ' ', 't', 'd', '[', ']'] {
            handle_key(
                &mut view,
                KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE),
                &state,
                &events,
                Some(&r),
                false,
                &dir,
            )
            .await
            .unwrap();
        }
        assert_eq!(r.store.lock().unwrap().events.len(), events.len());
        assert_eq!(r.state().autonomy, "normal");
        assert!(!r.busy());
    }
    #[test]
    fn normal_narrow_and_no_color_render_without_missing_essential_controls() {
        let tmp = tempfile::tempdir().unwrap();
        let r = Runtime::new(tmp.path(), Config::default(), true).unwrap();
        r.seed_demo().unwrap();
        r.demo_reveal().unwrap();
        let events = r.store.lock().unwrap().events.clone();
        for (w, h) in [(140, 44), (60, 24), (38, 14), (20, 8)] {
            let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
            let mut v = View::new();
            v.color = false;
            v.effects = "off".into();
            terminal
                .draw(|f| render(f, &r.state(), &events, &v))
                .unwrap();
            let text = terminal
                .backend()
                .buffer()
                .content
                .iter()
                .map(|c| c.symbol())
                .collect::<String>();
            assert!(text.contains("friendo"));
            if w >= 60 {
                assert!(text.contains("AUTONOMY"));
                assert!(text.contains("q quit"));
            }
        }
    }
    #[test]
    fn selection_positions_survive_new_events() {
        let mut view = View::new();
        let mut s = State::default();
        let a = Node::new("a", "a", "agent", "runtime", None, "");
        s.nodes.insert("a".into(), a);
        view.selection(&s, 0);
        s.nodes.insert(
            "z".into(),
            Node::new("z", "z", "process", "runtime", None, ""),
        );
        assert_eq!(view.selected.as_deref(), Some("a"));
    }
}
