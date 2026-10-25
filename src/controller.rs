use crate::{
    config::Config,
    model::{Kind, MCW_REVISION, Node, State},
    process::{self, Invocation, SessionManager},
    provider::{self, Message, ToolCall},
    store::Store,
};
use anyhow::{Context, Result, bail};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};
use tokio::{sync::mpsc, task::JoinHandle};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

#[derive(Clone)]
pub struct Runtime {
    pub store: Arc<Mutex<Store>>,
    pub config: Arc<Config>,
    pub preview: Arc<Mutex<String>>,
    sessions: Arc<tokio::sync::Mutex<SessionManager>>,
    cancel: Arc<Mutex<CancellationToken>>,
    busy: Arc<AtomicBool>,
    worker: Arc<tokio::sync::Mutex<Option<JoinHandle<()>>>>,
}
impl Runtime {
    pub fn new(root: &Path, config: Config, demo: bool) -> Result<Self> {
        let mut store = Store::create(root, config.secrets())?;
        store.append(
            "user_config",
            vec![],
            Kind::Started {
                demo,
                config: config.inspector(),
                source_revision: MCW_REVISION.into(),
            },
        )?;
        store.append(
            "user_config",
            vec![],
            Kind::Autonomy {
                mode: config.autonomy.clone(),
                boundary: "run start".into(),
            },
        )?;
        store.node(Node::new(
            "agent",
            "friendo / controller",
            "agent",
            "runtime",
            None,
            "Local controller. Graph connections never grant execution authority.",
        ))?;
        Ok(Self {
            store: Arc::new(Mutex::new(store)),
            config: Arc::new(config),
            preview: Arc::new(Mutex::new(String::new())),
            sessions: Arc::new(tokio::sync::Mutex::new(SessionManager::default())),
            cancel: Arc::new(Mutex::new(CancellationToken::new())),
            busy: Arc::new(AtomicBool::new(false)),
            worker: Arc::new(tokio::sync::Mutex::new(None)),
        })
    }
    pub fn state(&self) -> State {
        self.store.lock().expect("store poisoned").state.clone()
    }
    pub fn id(&self) -> String {
        self.store.lock().expect("store poisoned").id.clone()
    }
    pub fn busy(&self) -> bool {
        self.busy.load(Ordering::SeqCst)
    }
    pub fn event(&self, source: &str, kind: Kind) -> Result<String> {
        let mut s = self
            .store
            .lock()
            .map_err(|_| anyhow::anyhow!("journal lock poisoned"))?;
        let causes = if s.state.last_event.is_empty() {
            vec![]
        } else {
            vec![s.state.last_event.clone()]
        };
        s.append(source, causes, kind)
    }
    pub fn set_goal(&self, statement: &str, constraints: Vec<String>) -> Result<()> {
        if statement.len() > 64 * 1024 {
            bail!("goal exceeds 64 KiB");
        }
        if self.state().revision > 0 {
            self.correct(statement, "synchronization", None)?;
            return Ok(());
        }
        self.event(
            "user",
            Kind::Intent {
                revision: 1,
                statement: statement.into(),
                constraints,
            },
        )?;
        self.store.lock().unwrap().node(Node::new(
            "intent-1",
            statement,
            "intent",
            "coordination",
            Some(1),
            "Exact direct user statement / revision 1",
        ))?;
        Ok(())
    }
    pub fn cancel(&self, reason: &str) -> Result<()> {
        self.cancel.lock().unwrap().cancel();
        self.event(
            "user",
            Kind::CancelRequested {
                reason: reason.into(),
            },
        )?;
        Ok(())
    }
    pub fn correct(&self, statement: &str, operation: &str, target: Option<String>) -> Result<u64> {
        if statement.trim().is_empty() || statement.len() > 64 * 1024 {
            bail!("correction must contain 1..65536 bytes");
        }
        let revision;
        {
            let mut s = self.store.lock().unwrap();
            revision = s.state.revision + 1;
            let causes = target
                .as_ref()
                .and_then(|t| s.state.nodes.get(t))
                .map(|n| vec![n.event.clone()])
                .unwrap_or_default();
            s.append(
                "user",
                causes,
                Kind::Correction {
                    revision,
                    statement: statement.into(),
                    operation: operation.into(),
                    target,
                },
            )?;
            s.node(Node::new(&format!("intent-{revision}"),statement,"intent","coordination",Some(revision),"Exact direct user correction. Previous evidence retained; relevance must be reassessed."))?;
            if revision > 1 {
                s.edge(
                    &format!("intent-{revision}"),
                    &format!("intent-{}", revision - 1),
                    "supersedes",
                    "mechanically_derived",
                )?;
            }
            if !s.state.running.is_empty() {
                s.append("controller",vec![],Kind::Status{text:"Revision recorded after dispatch; already-running actions receive cancellation, prior side effects are not undone".into()})?;
            }
        }
        if self.busy() {
            self.cancel("intent revision changed; stop dependent turn")?;
        }
        Ok(revision)
    }
    pub fn autonomy(&self) -> Result<()> {
        let mode = if self.state().autonomy == "yolo" {
            "normal"
        } else {
            "yolo"
        };
        self.event(
            "user",
            Kind::Autonomy {
                mode: mode.into(),
                boundary: "next action dispatch; current action unchanged".into(),
            },
        )?;
        Ok(())
    }
    pub fn pin(&self, id: &str) -> Result<()> {
        let s = self.state();
        let node = s.nodes.get(id).context("selected node not found")?;
        let text = if node.kind == "artifact" {
            self.store
                .lock()
                .unwrap()
                .read_artifact(&node.detail)
                .context(
                    "source absent or too large; cannot silently pin a path instead of its content",
                )?
        } else {
            format!("{}\n{}", node.label, node.detail)
        };
        if text.len() > 65536 {
            bail!("pin exceeds 64KiB; select a bounded source excerpt");
        }
        self.event("user",Kind::Pin{node:id.into(),text,policy:"source content included verbatim (after credential redaction) in every subsequent model request; no truncation".into()})?;
        Ok(())
    }
    pub fn conflict(&self, text: &str) -> Result<()> {
        self.event(
            "user",
            Kind::Conflict {
                text: text.into(),
                basis: "human_report / possible interpretation conflict".into(),
                unresolved: true,
            },
        )?;
        Ok(())
    }
    pub fn acknowledge(&self) -> Result<()> {
        self.event("user",Kind::Acknowledged{revision:self.state().revision,text:"Direct human acknowledgment of the displayed recorded interpretation; no private understanding inferred".into()})?;
        Ok(())
    }
    pub fn launch_tool(&self, call: ToolCall) -> Result<()> {
        self.event(
            "user",
            Kind::Status {
                text: format!("DIRECT TOOL REQUEST / {}", serde_json::to_string(&call)?),
            },
        )?;
        if self.busy.swap(true, Ordering::SeqCst) {
            bail!("a turn is active; interrupt before a direct tool action");
        }
        let cancel = CancellationToken::new();
        *self.cancel.lock().unwrap() = cancel.clone();
        let rt = self.clone();
        tokio::spawn(async move {
            let result = rt
                .dispatch(&call, rt.state().revision, "agent", cancel)
                .await;
            match result {
                Ok(value) => {
                    let _ = rt.event(
                        "executor",
                        Kind::Status {
                            text: format!(
                                "DIRECT TOOL OBSERVED / {}",
                                excerpt(&value.to_string(), 8192)
                            ),
                        },
                    );
                }
                Err(error) => {
                    let _ = rt.event(
                        "executor",
                        Kind::Failure {
                            text: format!("{error:#}"),
                            outcome: "direct tool failed or blocked".into(),
                        },
                    );
                }
            }
            rt.busy.store(false, Ordering::SeqCst);
        });
        Ok(())
    }
    pub async fn interrupt_sessions(&self) -> Result<()> {
        let ids: Vec<_> = self.state().sessions.keys().cloned().collect();
        let mut sessions = self.sessions.lock().await;
        for id in ids {
            let before = sessions.read(&id).await?;
            if before.get("status").and_then(Value::as_str) != Some("completed") {
                sessions.interrupt(&id).await?;
            }
            let observed = sessions.read(&id).await?;
            self.event(
                "executor",
                Kind::ToolFinished {
                    id: format!("session-interrupt-{id}"),
                    result: observed,
                },
            )?;
        }
        Ok(())
    }
    pub fn constraints(&self, items: Vec<String>) -> Result<()> {
        if items.len() > 64 || items.iter().map(String::len).sum::<usize>() > 65536 {
            bail!("constraints exceed 64 entries or 64KiB");
        }
        let revision = self.correct(&self.state().goal, "re-grounding", None)?;
        self.event("user", Kind::Constraints { revision, items })?;
        Ok(())
    }
    pub fn launch(&self) -> Result<()> {
        if self.busy.swap(true, Ordering::SeqCst) {
            bail!("a turn is already active; interrupt it before another turn");
        }
        let token = CancellationToken::new();
        *self.cancel.lock().unwrap() = token.clone();
        let this = self.clone();
        let handle = tokio::spawn(async move {
            if let Err(e) = this.run_loop(token).await {
                let text = this.store.lock().unwrap().redact(&format!("{e:#}"));
                let _ = this.event(
                    "controller",
                    Kind::Failure {
                        text,
                        outcome: "incomplete".into(),
                    },
                );
            }
            this.preview.lock().unwrap().clear();
            this.busy.store(false, Ordering::SeqCst);
        });
        let worker = self.worker.clone();
        tokio::spawn(async move {
            *worker.lock().await = Some(handle);
        });
        Ok(())
    }
    pub async fn wait_idle(&self) {
        while self.busy() {
            tokio::time::sleep(std::time::Duration::from_millis(15)).await;
        }
    }
    pub async fn shutdown(&self) -> Result<()> {
        let mut persistence_error = None;
        if self.busy() {
            if let Err(error) = self.cancel("application quit") {
                persistence_error = Some(error);
            }
            self.wait_idle().await;
        }
        let outcomes = self.sessions.lock().await.shutdown().await?;
        for result in outcomes {
            self.event(
                "executor",
                Kind::ToolFinished {
                    id: "session-shutdown".into(),
                    result,
                },
            )?;
        }
        if let Some(error) = persistence_error {
            return Err(error);
        }
        Ok(())
    }
    fn ensure_revision(&self, revision: u64, cancel: &CancellationToken) -> Result<()> {
        if cancel.is_cancelled() {
            bail!("turn cancelled; incomplete output retained");
        }
        if self.state().revision != revision {
            bail!("stale turn withheld: intent revision changed");
        }
        Ok(())
    }
    pub fn tools() -> Value {
        json!([
            {"type":"function","function":{"name":"exec","description":"Direct native executable and argument array. No implicit shell. Admission follows configured executable policy.","parameters":{"type":"object","properties":{"executable":{"type":"string"},"args":{"type":"array","items":{"type":"string"}},"cwd":{"type":"string"},"stdin":{"type":"string"},"env_refs":{"type":"object","additionalProperties":{"type":"string"}},"timeout_ms":{"type":"integer"}},"required":["executable","args"],"additionalProperties":false}}},
            {"type":"function","function":{"name":"shell","description":"Explicit shell evaluation; requires execution.allow_shell and shell in executable policy.","parameters":{"type":"object","properties":{"shell":{"type":"string"},"expression":{"type":"string"},"cwd":{"type":"string"}},"required":["shell","expression"],"additionalProperties":false}}},
            {"type":"function","function":{"name":"session","description":"Persistent native pipe session, retained across model turns. No terminal emulation. start/read/write/interrupt/wait/terminate.","parameters":{"type":"object","properties":{"operation":{"type":"string","enum":["start","read","write","interrupt","wait","terminate"]},"id":{"type":"string"},"executable":{"type":"string"},"args":{"type":"array","items":{"type":"string"}},"cwd":{"type":"string"},"input":{"type":"string"},"timeout_ms":{"type":"integer"}},"required":["operation"],"additionalProperties":false}}}
        ])
    }
    async fn run_loop(&self, cancel: CancellationToken) -> Result<()> {
        let cfg = self.config.provider()?.clone();
        let initial = self.state();
        let revision = initial.revision;
        if revision == 0 {
            bail!("enter a goal first");
        }
        if initial.unresolved && initial.autonomy != "yolo" {
            self.event("controller",Kind::Status{text:"PAUSED / explicit unresolved interpretation conflict; Rewire or select YOLO to continue under recorded assumptions".into()})?;
            return Ok(());
        }
        if initial.unresolved {
            self.event("controller",Kind::Assumption{text:"Continue bounded work while the human-reported interpretation conflict remains unresolved".into(),basis:"explicit user-selected YOLO; execution policy and constraints unchanged".into()})?;
        }
        let mut messages=vec![Message{role:"system".into(),content:json!(self.config.system_prompt)},Message{role:"user".into(),content:json!({"intent_revision":revision,"exact_user_statement":initial.goal,"explicit_constraints":initial.constraints,"pinned_sources":initial.pins,"persistent_sessions":initial.sessions,"previous_interpretation":{"revision":initial.interpretation_revision,"text":initial.interpretation},"retained_answer_excerpt":initial.transcript,"uncertainty":"The previous answer may be stale. Re-evaluate against current intent; provider-internal context unavailable."}).to_string().into()}];
        for turn in 0..self.config.max_turns {
            self.ensure_revision(revision, &cancel)?;
            let id = format!("request-{}", Uuid::new_v4());
            let body = provider::request_body(&cfg, &messages, &Self::tools())?;
            let (context_id, context_path) = {
                let mut s = self.store.lock().unwrap();
                s.artifact("actual outbound provider request (credentials excluded)",&serde_json::to_vec_pretty(&body)?,false,"credential redaction only; tool results include bounded sanitized excerpts with raw artifact references")?
            };
            self.event(
                "controller",
                Kind::ModelRequested {
                    id: id.clone(),
                    revision,
                    context: context_path.clone(),
                },
            )?;
            {
                let mut s = self.store.lock().unwrap();
                if !s.state.nodes.contains_key("provider-active") {
                    s.node(Node::new("provider-active",&cfg.endpoint,"endpoint","runtime",None,&format!("Protocol: {}; model: {}; authentication uses credential reference only",cfg.protocol,cfg.model)))?;
                }
                s.node(
                    Node::new(
                        &id,
                        &format!("{} / turn {}", cfg.model, turn + 1),
                        "request",
                        "runtime",
                        Some(revision),
                        &format!(
                            "Actual outbound body: {context_path}; provider internals unavailable"
                        ),
                    )
                    .depends(&format!("intent-{revision}")),
                )?;
                s.node(Node::new(
                    &context_id,
                    "outbound context",
                    "artifact",
                    "evidence",
                    None,
                    &context_path,
                ))?;
                s.edge("agent", &id, "requested", "mechanically_derived")?;
                s.edge("provider-active", &id, "requested", "mechanically_derived")?;
                s.edge(&context_id, &id, "delivered_to", "mechanically_derived")?;
            }
            let (tx, mut rx) = mpsc::channel::<String>(32);
            let rt = self.clone();
            // Disposable streamed presentation is bounded; the final reply is authoritative.
            let display = tokio::spawn(async move {
                let mut preview = String::new();
                while let Some(chunk) = rx.recv().await {
                    preview.push_str(&chunk);
                    if preview.len() > 8192 {
                        let cut = preview.floor_char_boundary(preview.len() - 8192);
                        preview.drain(..cut);
                    }
                    let text = rt
                        .store
                        .lock()
                        .unwrap()
                        .redact(&process::sanitize(&preview));
                    *rt.preview.lock().unwrap() = text;
                }
            });
            let reply = provider::stream(&cfg, &messages, &Self::tools(), cancel.clone(), tx).await;
            display.await?;
            let reply = reply?;
            self.ensure_revision(revision, &cancel)?;
            self.event(
                "provider",
                Kind::Delivered {
                    revision,
                    request: id.clone(),
                },
            )?;
            let (_, response_path) = {
                let mut s = self.store.lock().unwrap();
                s.artifact("native provider continuation and complete response",&serde_json::to_vec_pretty(&json!({"text":reply.text,"tool_calls":reply.tool_calls,"continuation":reply.continuation,"usage":reply.usage}))?,false,"configured credential redaction")?
            };
            self.event(
                "provider",
                Kind::ModelFinished {
                    id: id.clone(),
                    text: excerpt(&excerpt(&process::sanitize(&reply.text), 16384), 65536),
                    response_artifact: response_path,
                    usage: reply.usage.clone(),
                },
            )?;
            self.event(
                "model",
                Kind::Interpretation {
                    revision,
                    text: excerpt(&excerpt(&process::sanitize(&reply.text), 16384), 8192),
                    basis: "model response; not inferred human acknowledgment".into(),
                },
            )?;
            {
                let mut store = self.store.lock().unwrap();
                let interpretation = if reply.text.is_empty() {
                    "No explicit textual interpretation supplied; tool requests retained separately"
                        .to_owned()
                } else {
                    excerpt(&process::sanitize(&reply.text), 8192)
                };
                store.node(
                    Node::new(
                        &format!("interpretation-{id}"),
                        "model interpretation",
                        "interpretation",
                        "coordination",
                        Some(revision),
                        &interpretation,
                    )
                    .depends(&format!("intent-{revision}")),
                )?;
                store.edge(
                    &id,
                    &format!("interpretation-{id}"),
                    "produced",
                    "mechanically_derived",
                )?;
                if !reply.tool_calls.is_empty() {
                    let calls: Vec<_> = reply
                        .tool_calls
                        .iter()
                        .map(|c| json!({"provider_call_id":c.id,"tool":c.name}))
                        .collect();
                    store.node(Node::new(&format!("plan-{id}"),"validated tool-call queue","plan","coordination",Some(revision),&format!("Mechanically derived queue from complete provider response; each action is rechecked before dispatch: {}",json!(calls))).depends(&format!("intent-{revision}")))?;
                    store.edge(
                        &id,
                        &format!("plan-{id}"),
                        "produced",
                        "mechanically_derived",
                    )?;
                }
            }
            if reply.tool_calls.is_empty() {
                let claim = format!("answer-{revision}-{}", Uuid::new_v4());
                let mut s = self.store.lock().unwrap();
                s.node(
                    Node::new(
                        &claim,
                        "answer / inspect evidence",
                        "claim",
                        "evidence",
                        Some(revision),
                        &excerpt(&process::sanitize(&reply.text), 16384),
                    )
                    .depends(&format!("intent-{revision}")),
                )?;
                s.edge(&id, &claim, "produced", "mechanically_derived")?;
                drop(s);
                self.refresh(revision,"provider response re-evaluated current intent; semantic correctness remains unverified")?;
                self.event(
                    "controller",
                    Kind::Status {
                        text: "TURN COMPLETE / model answer retained; cost unknown".into(),
                    },
                )?;
                return Ok(());
            }
            messages.push(Message{role:"assistant".into(),content:json!({"text":reply.text,"tool_calls":reply.tool_calls,"native":reply.continuation})});
            for call in &reply.tool_calls {
                self.ensure_revision(revision, &cancel)?;
                let result = self.dispatch(call, revision, &id, cancel.clone()).await;
                let content = match result {
                    Ok(v) => v,
                    Err(e) => {
                        let text = format!("{e:#}");
                        self.event(
                            "executor",
                            Kind::Failure {
                                text: text.clone(),
                                outcome: "blocked or failed tool".into(),
                            },
                        )?;
                        json!({"error":text})
                    }
                };
                messages.push(Message {
                    role: "tool".into(),
                    content: json!({"tool_call_id":call.id,"content":content.to_string()}),
                });
            }
        }
        bail!("configured model turn budget reached; no completion inferred")
    }
    pub fn refresh(&self, revision: u64, basis: &str) -> Result<()> {
        let state = self.state();
        if state.revision != revision {
            bail!("cannot refresh stale revision");
        }
        let affected: Vec<_> = state
            .nodes
            .values()
            .filter(|n| n.stale && n.revision == Some(revision))
            .map(|n| n.id.clone())
            .collect();
        for node in &affected {
            self.event(
                "controller",
                Kind::Refreshed {
                    node: node.clone(),
                    revision,
                    basis: basis.into(),
                },
            )?;
        }
        self.event("mechanical_check",Kind::Check{id:"revision-delivery".into(),revision,passed:self.state().delivered_revision==revision&&self.state().interpretation_revision==revision,condition:"context delivery and recorded interpretation reference the current intent revision".into(),evidence:vec![self.state().last_event]})?;
        self.event(
            "controller",
            Kind::Repair {
                operation: "synchronization".into(),
                revision,
                receipt: format!(
                    "REVISION DELIVERED / {} EXPLICIT REFRESHES / {} DECLARED CHECKS PENDING / historical work retained",
                    affected.len(),
                    self.state().pending_checks()
                ),
            },
        )?;
        // Live semantic evidence is never promoted into a success claim by animation.
        Ok(())
    }
    pub async fn execute_direct(&self, executable: String, args: Vec<String>) -> Result<Value> {
        let cancel = CancellationToken::new();
        *self.cancel.lock().unwrap() = cancel.clone();
        let call = ToolCall {
            id: Uuid::new_v4().to_string(),
            name: "exec".into(),
            arguments: json!({"executable":executable,"args":args}),
        };
        let future = self.dispatch(&call, self.state().revision, "agent", cancel.clone());
        tokio::pin!(future);
        tokio::select! {result=&mut future=>result,_=tokio::signal::ctrl_c()=>{self.cancel("direct command Ctrl-C")?;future.await}}
    }
    async fn dispatch(
        &self,
        call: &ToolCall,
        revision: u64,
        request: &str,
        cancel: CancellationToken,
    ) -> Result<Value> {
        if !call.arguments.is_object() {
            bail!("tool arguments must be an object");
        }
        let args = &call.arguments;
        let allowed_fields: &[&str] = match call.name.as_str() {
            "exec" => &[
                "executable",
                "args",
                "cwd",
                "stdin",
                "env_refs",
                "timeout_ms",
            ],
            "shell" => &["shell", "expression", "cwd"],
            "session" => &[
                "operation",
                "id",
                "executable",
                "args",
                "cwd",
                "input",
                "timeout_ms",
            ],
            _ => bail!("unknown tool {}", call.name),
        };
        for k in args.as_object().unwrap().keys() {
            if !allowed_fields.contains(&k.as_str()) {
                bail!("unsupported tool field {k}");
            }
        }
        let invocation = if call.name == "exec"
            || call.name == "shell"
            || args.get("operation").and_then(Value::as_str) == Some("start")
        {
            Some(self.invocation(&call.name, args)?)
        } else {
            None
        };
        let id = format!("tool-{}-{}", call.id, Uuid::new_v4());
        {
            let mut s = self.store.lock().unwrap();
            if cancel.is_cancelled() || s.state.revision != revision {
                bail!("queued dependent action is stale or cancelled; dispatch withheld");
            }
            if s.state.unresolved && s.state.autonomy != "yolo" {
                bail!("dependent action paused for unresolved human-reported conflict");
            }
            let cause = s.state.last_event.clone();
            s.append("controller",vec![cause],Kind::ToolStarted{id:id.clone(),revision,invocation:json!({"tool_call_id":call.id,"tool":call.name,"arguments":call.arguments,"environment":"only explicit env references retained"})})?;
            s.node(
                Node::new(
                    &id,
                    &format!("{} / {}", call.name, call.id),
                    if call.name == "session" {
                        "session"
                    } else {
                        "process"
                    },
                    "runtime",
                    Some(revision),
                    &call.arguments.to_string(),
                )
                .depends(&format!("intent-{revision}")),
            )?;
            s.edge(request, &id, "invokes", "mechanically_derived")?;
        }
        let dir = self
            .store
            .lock()
            .unwrap()
            .dir
            .join("artifacts")
            .join(format!("process-{}", Uuid::new_v4()));
        let result:Result<Value>=async {
            if call.name=="session" {
                let op=string(args,"operation")?;let mut mgr=self.sessions.lock().await;
                let sid=args.get("id").and_then(Value::as_str).unwrap_or("");
                match op {
                    "start"=>{let sid=mgr.start(invocation.context("missing session invocation")?,dir).await?;mgr.read(&sid).await},
                    "read"=>mgr.read(sid).await,
                    "write"=>{tokio::select!{result=mgr.write(sid,string(args,"input")?)=>result?,_=cancel.cancelled()=>bail!("session write cancelled; delivery may be partial")};Ok(json!({"session_id":sid,"status":"input written"}))},
                    "interrupt"=>{mgr.interrupt(sid).await?;Ok(json!({"session_id":sid,"status":"interrupt requested; read/wait for observation"}))},
                    "wait"=>{let timeout=args.get("timeout_ms").and_then(Value::as_u64).unwrap_or(1000).min(5000);tokio::select!{r=mgr.wait(sid,timeout)=>r,_=cancel.cancelled()=>bail!("session wait cancelled; session remains available until shutdown")}},
                    "terminate"=>mgr.terminate(sid).await,
                    _=>bail!("unknown session operation"),
                }
            } else {
                let result=process::execute(invocation.context("missing invocation")?,dir,cancel.clone()).await?;
                Ok(serde_json::to_value(result)?)
            }
        }.await;
        let value = match &result {
            Ok(v) => v.clone(),
            Err(e) => {
                json!({"error":format!("{e:#}"),"outcome":"failure or unknown; inspect executor evidence"})
            }
        };
        self.event(
            "executor",
            Kind::ToolFinished {
                id: id.clone(),
                result: value.clone(),
            },
        )?;
        if let Ok(v) = &result {
            let mut s = self.store.lock().unwrap();
            for stream in ["stdout", "stderr"] {
                if let Some(path) = v.get(format!("{stream}_path")).and_then(Value::as_str) {
                    let aid = format!("{id}-{stream}");
                    s.node(Node::new(
                        &aid,
                        &format!("{stream} artifact"),
                        "artifact",
                        "evidence",
                        None,
                        path,
                    ))?;
                    s.edge(&id, &aid, "produced", "mechanically_derived")?;
                    s.append("executor",vec![],Kind::Artifact{id:aid,path:path.into(),label:format!("raw {stream}; excerpts sanitized and bounded"),bytes:v.get(format!("{stream}_bytes")).and_then(Value::as_u64).unwrap_or(0),synthetic:false,transformation:"process capture; model receives exact recorded excerpt in next outbound request".into()})?;
                }
            }
        }
        result
    }
    fn invocation(&self, name: &str, args: &Value) -> Result<Invocation> {
        let (executable, argv) = if name == "shell" {
            if !self.config.execution.allow_shell {
                bail!("shell evaluation disabled by execution policy");
            }
            let shell = string(args, "shell")?;
            let expression = string(args, "expression")?;
            let lower = shell.to_lowercase();
            let argv = if lower.ends_with("cmd") || lower.ends_with("cmd.exe") {
                vec!["/C".into(), expression.into()]
            } else if lower.contains("powershell") || lower.contains("pwsh") {
                vec!["-NoProfile".into(), "-Command".into(), expression.into()]
            } else {
                vec!["-c".into(), expression.into()]
            };
            (shell.to_owned(), argv)
        } else {
            let exe = string(args, "executable")?.to_owned();
            let argv = args
                .get("args")
                .and_then(Value::as_array)
                .context("args must be an array")?
                .iter()
                .map(|v| {
                    v.as_str()
                        .map(str::to_owned)
                        .context("each argument must be a string")
                })
                .collect::<Result<Vec<_>>>()?;
            (exe, argv)
        };
        if !self.config.allows(&executable) {
            bail!(
                "executable {executable:?} is outside configured execution.allowed_executables; YOLO does not change this policy"
            );
        }
        let mut env = BTreeMap::new();
        if let Some(refs) = args.get("env_refs") {
            for (k, v) in refs.as_object().context("env_refs must be an object")? {
                let reference = v.as_str().context("env reference must be a string")?;
                if self
                    .config
                    .providers
                    .values()
                    .filter_map(|p| p.api_key_env.as_deref())
                    .any(|secret| secret == k || secret == reference)
                {
                    bail!("provider credential references cannot be passed to tools");
                }
                env.insert(
                    k.clone(),
                    std::env::var(v.as_str().context("env reference must be a string")?)
                        .context("referenced environment variable missing")?,
                );
            }
        }
        let timeout = args
            .get("timeout_ms")
            .map(|v| v.as_u64().context("timeout_ms must be unsigned integer"))
            .transpose()?
            .unwrap_or(self.config.execution.timeout_ms)
            .min(self.config.execution.timeout_ms);
        if timeout == 0 {
            bail!("timeout_ms must be positive; model input cannot disable the deadline");
        }
        let cwd = args
            .get("cwd")
            .map(|v| v.as_str().context("cwd must be a string"))
            .transpose()?
            .map(PathBuf::from)
            .unwrap_or(std::env::current_dir()?);
        let mut redact = self.config.secrets();
        redact.extend(env.values().cloned());
        Ok(Invocation {
            executable,
            args: argv,
            cwd,
            stdin: args
                .get("stdin")
                .map(|v| {
                    v.as_str()
                        .map(str::to_owned)
                        .context("stdin must be a string")
                })
                .transpose()?,
            env,
            timeout_ms: timeout,
            output_limit_bytes: self.config.execution.output_limit_bytes,
            redact,
            env_remove: self
                .config
                .providers
                .values()
                .filter_map(|p| p.api_key_env.clone())
                .collect(),
        })
    }
    pub fn seed_demo(&self) -> Result<()> {
        self.set_goal(
            "Inventory symbols in the synthetic local binary fixture",
            vec!["Preserve the original binary; do not modify it".into()],
        )?;
        let mut s = self.store.lock().unwrap();
        s.node(
            Node::new(
                "interpretation",
                "all symbols",
                "interpretation",
                "coordination",
                Some(1),
                "Interpretation: include exported and internal symbols",
            )
            .depends("intent-1")
            .synthetic(),
        )?;
        s.node(
            Node::new(
                "plan",
                "symbol inventory",
                "plan",
                "coordination",
                Some(1),
                "Read synthetic symbols; never modify the original binary",
            )
            .depends("interpretation")
            .synthetic(),
        )?;
        s.node(
            Node::new(
                "host",
                "fixture-host / SYNTHETIC",
                "host",
                "domain",
                None,
                "Imported fixture / no live discovery / 192.0.2.10 (documentation address)",
            )
            .synthetic(),
        )?;
        s.node(
            Node::new(
                "service",
                "symbols:31337 / SYNTHETIC",
                "service",
                "domain",
                None,
                "Synthetic observed service; no network access occurs",
            )
            .synthetic(),
        )?;
        s.edge("host", "service", "observed_link", "synthetic_import")?;
        s.edge("intent-1", "interpretation", "requested", "declared")?;
        drop(s);
        self.event(
            "demo_fixture",
            Kind::Interpretation {
                revision: 1,
                text: "Inventory all symbols, exported and internal; preserve the original binary"
                    .into(),
                basis: "explicitly synthetic scripted agent interpretation".into(),
            },
        )?;
        self.event(
            "demo_fixture",
            Kind::Status {
                text: "OFFLINE DEMO / press Space to reveal the invocation and supported answer"
                    .into(),
            },
        )?;
        Ok(())
    }
    pub fn demo_reveal(&self) -> Result<()> {
        if self.state().nodes.contains_key("answer") {
            return Ok(());
        }
        self.event("demo_fixture",Kind::ToolStarted{id:"demo-invocation".into(),revision:1,invocation:json!({"executable":"friendo-fixture","args":["symbols"],"synthetic":true,"execution":"recorded deterministic fixture; no external execution"})})?;
        let mut s = self.store.lock().unwrap();
        let (aid, path) = s.artifact(
            "synthetic symbol output",
            b"EXPORT friendo_fixture_exported\nLOCAL friendo_fixture_internal\n",
            true,
            "none; synthetic retained source",
        )?;
        s.node(
            Node::new(
                "demo-invocation",
                "symbol reader / SYNTHETIC",
                "process",
                "runtime",
                Some(1),
                "Direct invocation fixture (offline demonstration)",
            )
            .depends("plan")
            .synthetic(),
        )?;
        s.node(
            Node::new(
                &aid,
                "symbol output / SYNTHETIC",
                "artifact",
                "evidence",
                None,
                &path,
            )
            .synthetic(),
        )?;
        s.node(Node::new("answer","2 symbols / SYNTHETIC","claim","evidence",Some(1),"Exported: friendo_fixture_exported. Internal: friendo_fixture_internal. Supported by retained fixture.").depends("plan").synthetic())?;
        s.edge("plan", "demo-invocation", "invokes", "synthetic_fixture")?;
        s.edge("demo-invocation", &aid, "produced", "synthetic_fixture")?;
        s.edge(&aid, "answer", "supports", "declared")?;
        drop(s);
        self.event(
            "demo_fixture",
            Kind::ToolFinished {
                id: "demo-invocation".into(),
                result: json!({"exit_code":0,"synthetic":true,"artifact":path}),
            },
        )?;
        self.event("demo_fixture",Kind::ModelFinished{id:"demo-response".into(),text:"Two synthetic symbols: exported friendo_fixture_exported and internal friendo_fixture_internal. Original binary preserved.".into(),response_artifact:path,usage:None})?;
        self.event(
            "demo_fixture",
            Kind::Status {
                text: "OFFLINE DEMO / Space changes the goal to exported symbols only".into(),
            },
        )?;
        Ok(())
    }
    pub fn demo_correct(&self) -> Result<()> {
        if self.state().revision == 1 {
            self.correct(
                "Inventory exported symbols only; preserve the original binary",
                "disambiguation",
                Some("answer".into()),
            )?;
        }
        Ok(())
    }
    pub fn demo_repair(&self, statement: &str, operation: &str) -> Result<()> {
        if statement != "Inventory exported symbols only; preserve the original binary" {
            bail!(
                "guided synthetic fixture supports the displayed exported-symbol correction; arbitrary corrections require live run"
            );
        }
        let before = self.state();
        let revision = if before.revision < 2 || statement != before.goal {
            self.correct(statement, operation, Some("answer".into()))?
        } else {
            before.revision
        };
        let context = json!({"exact_statement":self.state().goal,"constraints":self.state().constraints,"pins":self.state().pins,"retained_sources":self.state().artifacts,"synthetic":true});
        let (_, path) = self.store.lock().unwrap().artifact(
            "synthetic outbound correction context",
            &serde_json::to_vec_pretty(&context)?,
            true,
            "none; deterministic offline context",
        )?;
        self.event(
            "demo_fixture",
            Kind::ModelRequested {
                id: "demo-repair".into(),
                revision,
                context: path,
            },
        )?;
        self.event(
            "demo_fixture",
            Kind::Delivered {
                revision,
                request: "demo-repair".into(),
            },
        )?;
        self.event("demo_fixture",Kind::Interpretation{revision,text:format!("Current exact correction: {statement}. Read exported symbols only; retain original-binary constraint."),basis:"synthetic deterministic fixture; no inference about human private understanding".into()})?;
        let mut s = self.store.lock().unwrap();
        let text = "EXPORT friendo_fixture_exported\n";
        let (aid, path) = s.artifact(
            "refreshed exported-symbol evidence",
            text.as_bytes(),
            true,
            "deterministic filter: retain lines with EXPORT prefix",
        )?;
        s.node(
            Node::new(
                &aid,
                "exports / SYNTHETIC",
                "artifact",
                "evidence",
                None,
                &path,
            )
            .synthetic(),
        )?;
        s.node(
            Node::new(
                "repaired-answer",
                "1 exported symbol / SYNTHETIC",
                "claim",
                "evidence",
                Some(revision),
                "friendo_fixture_exported; internal symbols omitted; original retained.",
            )
            .depends(&format!("intent-{revision}"))
            .synthetic(),
        )?;
        s.edge(
            &aid,
            "repaired-answer",
            "supports",
            "synthetic_mechanical_filter",
        )?;
        drop(s);
        self.event("demo_fixture",Kind::ModelFinished{id:"demo-repaired-answer".into(),text:"One exported synthetic symbol: friendo_fixture_exported. Internal symbol excluded; original fixture retained.".into(),response_artifact:path,usage:None})?;
        {
            let mut s = self.store.lock().unwrap();
            s.node(
                Node::new(
                    "interpretation",
                    "exported symbols",
                    "interpretation",
                    "coordination",
                    Some(revision),
                    "Updated interpretation: exported symbols only; original binary preserved",
                )
                .depends(&format!("intent-{revision}"))
                .synthetic(),
            )?;
            s.node(
                Node::new(
                    "plan",
                    "export inventory",
                    "plan",
                    "coordination",
                    Some(revision),
                    "Read retained fixture; select EXPORT rows; preserve original source",
                )
                .depends("interpretation")
                .synthetic(),
            )?;
        }
        // Only named current planning nodes are re-evaluated. Original answer remains stale.
        for id in ["interpretation", "plan"] {
            self.event(
                "demo_fixture",
                Kind::Refreshed {
                    node: id.into(),
                    revision,
                    basis: "scripted interpretation and plan updated from retained source".into(),
                },
            )?;
        }
        self.event(
            "mechanical_check",
            Kind::Check {
                id: "exported-only".into(),
                revision,
                passed: !text.contains("LOCAL") && text.starts_with("EXPORT"),
                condition: "refreshed fixture evidence contains EXPORT rows and no LOCAL rows"
                    .into(),
                evidence: vec![aid],
            },
        )?;
        let original_preserved = {
            let store = self.store.lock().unwrap();
            store
                .events
                .iter()
                .find_map(|e| {
                    if let Kind::Artifact { path, label, .. } = &e.kind {
                        if label == "synthetic symbol output" {
                            store.read_artifact(path).ok()
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                })
                .is_some_and(|text| {
                    text == "EXPORT friendo_fixture_exported\nLOCAL friendo_fixture_internal\n"
                })
        };
        self.event("mechanical_check",Kind::Check{id:"original-preserved".into(),revision,passed:original_preserved,condition:"offline fixture source artifact still retained byte for byte; no binary modification tool dispatched".into(),evidence:vec!["demo-invocation".into()]})?;
        self.event("user",Kind::Repair{operation:operation.into(),revision,receipt:"REVISION APPLIED / 2 DEPENDENCIES REFRESHED / 0 DECLARED CHECKS PENDING / old answer remains stale".into()})?;
        let state = self.state();
        if state.delivered_revision == revision
            && state.interpretation_revision == revision
            && state.checks.values().all(|p| *p)
        {
            self.event(
                "mechanical_check",
                Kind::Restored {
                    revision,
                    conditions: vec![
                        "context delivered at current revision".into(),
                        "interpretation updated at current revision".into(),
                        "exported-only fixture check passed".into(),
                        "original source retention checked".into(),
                    ],
                    evidence: vec!["exported-only".into(), "original-preserved".into()],
                },
            )?;
        }
        self.event("demo_fixture",Kind::Status{text:"OFFLINE DEMO / repair recorded. y toggles YOLO; [ reviews earlier events; Enter inspects the receipt".into()})?;
        Ok(())
    }
}
fn string<'a>(v: &'a Value, key: &str) -> Result<&'a str> {
    v.get(key)
        .and_then(Value::as_str)
        .with_context(|| format!("{key} must be a string"))
}
pub fn excerpt(s: &str, limit: usize) -> String {
    if s.len() <= limit {
        s.into()
    } else {
        format!(
            "{}\n[excerpt truncated; full retained artifact]",
            &s[..s.floor_char_boundary(limit)]
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn demo_repairs_actual_state_and_preserves_constraints() {
        let t = tempfile::tempdir().unwrap();
        let r = Runtime::new(t.path(), Config::default(), true).unwrap();
        r.seed_demo().unwrap();
        r.demo_reveal().unwrap();
        let artifacts = r.state().artifacts;
        r.demo_correct().unwrap();
        assert!(r.state().nodes["answer"].stale);
        let goal = r.state().goal;
        r.demo_repair(&goal, "disambiguation").unwrap();
        let s = r.state();
        assert_eq!(s.revision, 2);
        assert_eq!(s.delivered_revision, 2);
        assert!(s.restored);
        assert_eq!(s.constraints.len(), 1);
        assert!(s.nodes["answer"].stale);
        for id in artifacts.keys() {
            assert!(s.artifacts.contains_key(id));
        }
        r.autonomy().unwrap();
        assert_eq!(r.state().autonomy, "yolo");
        assert_eq!(r.state().constraints.len(), 1);
        assert!(s.nodes["host"].synthetic);
    }
    #[test]
    fn policy_independent_of_yolo() {
        let c = Config {
            autonomy: "yolo".into(),
            ..Config::default()
        };
        assert!(!c.allows("sh"));
        assert!(!c.execution.allow_shell);
    }
    #[test]
    fn model_cannot_remove_or_extend_deadline() {
        let root = tempfile::tempdir().unwrap();
        let runtime = Runtime::new(root.path(), Config::default(), false).unwrap();
        assert!(
            runtime
                .invocation(
                    "exec",
                    &json!({"executable":"echo","args":[],"timeout_ms":0})
                )
                .is_err()
        );
        let shorter = runtime
            .invocation(
                "exec",
                &json!({"executable":"echo","args":[],"timeout_ms":10}),
            )
            .unwrap();
        assert_eq!(shorter.timeout_ms, 10);
        let longer = runtime
            .invocation(
                "exec",
                &json!({"executable":"echo","args":[],"timeout_ms":u64::MAX}),
            )
            .unwrap();
        assert_eq!(longer.timeout_ms, runtime.config.execution.timeout_ms);
    }
    #[test]
    fn provider_environment_is_rejected_for_destination_and_reference_names() {
        let root = tempfile::tempdir().unwrap();
        let credential = "FRIENDO_TEST_PROVIDER_CREDENTIAL";
        let config = Config {
            providers: BTreeMap::from([(
                "fixture".into(),
                provider::ProviderConfig {
                    model: "explicit-fixture".into(),
                    api_key_env: Some(credential.into()),
                    ..provider::ProviderConfig::default()
                },
            )]),
            ..Config::default()
        };
        let runtime = Runtime::new(root.path(), config, false).unwrap();
        for refs in [
            json!({credential: "PATH"}),
            json!({"ORDINARY_DESTINATION": credential}),
        ] {
            let error = runtime
                .invocation(
                    "exec",
                    &json!({"executable":"echo","args":[],"env_refs":refs}),
                )
                .unwrap_err();
            assert!(
                error
                    .to_string()
                    .contains("provider credential references cannot be passed"),
                "{error:#}"
            );
        }
        // PATH is inherited on supported hosts; no process-global environment mutation is needed.
        let ordinary = runtime
            .invocation(
                "exec",
                &json!({"executable":"echo","args":[],"env_refs":{"ORDINARY_DESTINATION":"PATH"}}),
            )
            .unwrap();
        assert_eq!(
            ordinary.env["ORDINARY_DESTINATION"],
            std::env::var("PATH").unwrap()
        );
        assert!(
            ordinary
                .redact
                .contains(&ordinary.env["ORDINARY_DESTINATION"])
        );
        assert_eq!(ordinary.env_remove, [credential]);
        let missing = format!("FRIENDO_MISSING_{}", Uuid::new_v4().simple());
        let error = runtime
            .invocation(
                "exec",
                &json!({"executable":"echo","args":[],"env_refs":{"DEST":missing}}),
            )
            .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("referenced environment variable missing")
        );
    }
    #[tokio::test]
    async fn rejected_tools_never_reach_execution_or_append_tool_started() {
        let root = tempfile::tempdir().unwrap();
        let runtime = Runtime::new(root.path(), Config::default(), false).unwrap();
        runtime.set_goal("reject invalid calls", vec![]).unwrap();
        let count = runtime.store.lock().unwrap().events.len();
        let cases = [
            ("exec", json!([]), "must be an object"),
            ("unknown", json!({}), "unknown tool"),
            (
                "exec",
                json!({"executable":"echo","args":[],"allow_shell":true}),
                "unsupported tool field",
            ),
            (
                "exec",
                json!({"executable":"echo","args":"wrong"}),
                "args must be an array",
            ),
            (
                "exec",
                json!({"executable":"echo","args":[7]}),
                "each argument must be a string",
            ),
            (
                "exec",
                json!({"executable":7,"args":[]}),
                "executable must be a string",
            ),
            (
                "exec",
                json!({"executable":"echo","args":[],"stdin":7}),
                "stdin must be a string",
            ),
            (
                "exec",
                json!({"executable":"echo","args":[],"cwd":7}),
                "cwd must be a string",
            ),
            (
                "exec",
                json!({"executable":"echo","args":[],"timeout_ms":-1}),
                "timeout_ms must be unsigned",
            ),
            (
                "exec",
                json!({"executable":"echo","args":[],"env_refs":[]}),
                "env_refs must be an object",
            ),
            (
                "exec",
                json!({"executable":"echo","args":[],"env_refs":{"NAME":7}}),
                "env reference must be a string",
            ),
            (
                "exec",
                json!({"executable":"unadmitted-fixture","args":[]}),
                "outside configured",
            ),
            (
                "shell",
                json!({"shell":"sh","expression":"echo forbidden"}),
                "shell evaluation disabled",
            ),
        ];
        for (name, arguments, expected) in cases {
            let call = ToolCall {
                id: "invalid".into(),
                name: name.into(),
                arguments,
            };
            let error = runtime
                .dispatch(
                    &call,
                    runtime.state().revision,
                    "agent",
                    CancellationToken::new(),
                )
                .await
                .unwrap_err();
            assert!(format!("{error:#}").contains(expected), "{name}: {error:#}");
            assert_eq!(
                runtime.store.lock().unwrap().events.len(),
                count,
                "rejection must precede ToolStarted"
            );
        }
    }
    #[test]
    fn cancellation_still_signals_when_journal_budget_is_exhausted() {
        let root = tempfile::tempdir().unwrap();
        let r = Runtime::new(root.path(), Config::default(), false).unwrap();
        {
            let mut store = r.store.lock().unwrap();
            let event = crate::model::Event {
                schema: 1,
                id: String::new(),
                run_id: String::new(),
                branch_id: String::new(),
                seq: 0,
                timestamp: String::new(),
                source: String::new(),
                causes: vec![],
                kind: Kind::Status {
                    text: String::new(),
                },
            };
            store.events.resize(100_000, event);
        }
        assert!(r.cancel("test failed persistence").is_err());
        assert!(r.cancel.lock().unwrap().is_cancelled());
    }
    #[test]
    fn explicit_constraints_change_only_through_user_transition() {
        let root = tempfile::tempdir().unwrap();
        let r = Runtime::new(root.path(), Config::default(), false).unwrap();
        r.set_goal("task", vec!["original".into()]).unwrap();
        r.correct("new goal", "synchronization", None).unwrap();
        assert_eq!(r.state().constraints, vec!["original"]);
        r.constraints(vec!["replacement".into()]).unwrap();
        assert_eq!(r.state().revision, 3);
        assert_eq!(r.state().constraints, vec!["replacement"]);
        r.acknowledge().unwrap();
        assert_eq!(r.state().acknowledged_revision, Some(3));
    }
}
