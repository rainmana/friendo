use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use friendo::{
    config::Config,
    controller::Runtime,
    model::{Kind, State},
    store::{self, recover},
    tui::{self, View},
};
use ratatui::{Terminal, backend::TestBackend};
use std::{
    io::Write,
    path::{Path, PathBuf},
};

#[derive(Parser)]
#[command(
    name = "friendo",
    version,
    about = "Native agent console. Durable evidence. Direct control."
)]
struct Cli {
    #[arg(long, global = true)]
    config: Option<PathBuf>,
    #[arg(long, global = true, default_value = ".friendo/runs")]
    data_dir: PathBuf,
    #[arg(long, global = true)]
    yolo: bool,
    #[arg(long,global=true,value_parser=["off","reduced","full","overdrive"])]
    effects: Option<String>,
    #[arg(long, global = true)]
    no_color: bool,
    #[command(subcommand)]
    command: Commands,
}
#[derive(Subcommand)]
enum Commands {
    /// Credential-free guided synthetic workflow; Space steps, r repairs.
    Demo {
        #[arg(long)]
        headless: bool,
        #[arg(long)]
        snapshot: Option<PathBuf>,
        #[arg(long, default_value_t = 140)]
        width: u16,
        #[arg(long, default_value_t = 44)]
        height: u16,
    },
    /// Stream a model/tool loop; omit goal to compose in the terminal.
    Run {
        goal: Option<String>,
        #[arg(long)]
        headless: bool,
        #[arg(long)]
        provider: Option<String>,
        #[arg(long)]
        model: Option<String>,
        #[arg(long)]
        constraint: Vec<String>,
    },
    /// List local recorded runs without executing them.
    Runs,
    /// Read-only historical review; this never constructs an executor.
    Replay {
        run_id: String,
        #[arg(long)]
        headless: bool,
        #[arg(long)]
        at: Option<usize>,
        #[arg(long)]
        context: bool,
        #[arg(long)]
        snapshot: Option<PathBuf>,
        #[arg(long, default_value_t = 140)]
        width: u16,
        #[arg(long, default_value_t = 44)]
        height: u16,
    },
    /// Inspect effective configuration and provider readiness. No model call.
    Doctor,
    /// Run a direct native command through the same journal and executor policy.
    Exec {
        executable: String,
        #[arg(last = true)]
        args: Vec<String>,
    },
}
#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let mut config = Config::load(cli.config.as_deref())?;
    if cli.yolo {
        config.autonomy = "yolo".into();
        config
            .overrides
            .insert("autonomy".into(), "CLI --yolo".into());
    }
    if let Some(e) = cli.effects {
        config.effects = e;
        config
            .overrides
            .insert("effects".into(), "CLI --effects".into());
    }
    if cli.no_color {
        config.color = false;
        config
            .overrides
            .insert("color".into(), "CLI --no-color".into());
    }
    match cli.command {
        Commands::Doctor => {
            println!("friendo {} / local readiness", env!("CARGO_PKG_VERSION"));
            println!(
                "platform: {} / {}",
                std::env::consts::OS,
                std::env::consts::ARCH
            );
            println!("data directory: {}", cli.data_dir.display());
            for (alias, p) in &config.providers {
                let ready = p
                    .api_key_env
                    .as_ref()
                    .map(|k| std::env::var(k).is_ok())
                    .unwrap_or(true);
                println!(
                    "provider {alias}: {} / model {} / credential {} / streaming not contacted",
                    p.protocol,
                    p.model,
                    if ready {
                        "available or not required"
                    } else {
                        "MISSING"
                    }
                );
            }
            if config.providers.is_empty() {
                println!(
                    "No providers configured. Demo and direct exec work offline. See friendo.example.toml."
                );
            }
            println!("Pipe sessions: supported / PTY emulator: unavailable / price: unknown");
            println!("{}", serde_json::to_string_pretty(&config.inspector())?);
        }
        Commands::Runs => {
            if cli.data_dir.exists() {
                let mut dirs = std::fs::read_dir(&cli.data_dir)?
                    .filter_map(|e| e.ok())
                    .collect::<Vec<_>>();
                dirs.sort_by_key(|d| d.file_name());
                for d in dirs {
                    if d.path().join("events.jsonl").exists() {
                        match recover(&d.path().join("events.jsonl")) {
                            Ok(r) => {
                                let s = State::at(&r.events);
                                println!(
                                    "{}  r{}  {}  {}  {} warnings",
                                    d.file_name().to_string_lossy(),
                                    s.revision,
                                    if s.demo { "SYNTHETIC" } else { "LIVE" },
                                    s.goal.replace('\n', " "),
                                    r.warnings.len()
                                );
                            }
                            Err(e) => eprintln!("{}: {e:#}", d.file_name().to_string_lossy()),
                        }
                    }
                }
            }
        }
        Commands::Replay {
            run_id,
            headless,
            at,
            context,
            snapshot,
            width,
            height,
        } => {
            let dir = store::resolve_run(&cli.data_dir, &run_id)?;
            let recovered = recover(&dir.join("events.jsonl"))?;
            for w in &recovered.warnings {
                eprintln!("RECOVERY / {w}");
            }
            let mut events = recovered.events;
            if let Some(at) = at {
                events.truncate(at);
            }
            let state = State::at(&events);
            if context {
                if state.last_context.is_empty() {
                    bail!("no outbound context retained at selected position");
                }
                let path = dir.join(&state.last_context);
                let canonical = path.canonicalize()?;
                if !canonical.starts_with(dir.canonicalize()?)
                    || std::fs::metadata(&canonical)?.len() > 16 * 1024 * 1024
                {
                    bail!("invalid or oversized context artifact");
                }
                println!("{}", std::fs::read_to_string(canonical)?);
            } else if let Some(path) = snapshot {
                capture(&path, width, height, &state, &events, config.color)?;
                println!("snapshot: {}", path.display());
            } else if headless {
                println!("{}", serde_json::to_string_pretty(&state)?);
            } else {
                tui::run(None, events, dir, true).await?;
            }
        }
        Commands::Demo {
            headless,
            snapshot,
            width,
            height,
        } => {
            let runtime = Runtime::new(&cli.data_dir, config, true)?;
            runtime.seed_demo()?;
            let result = if headless || snapshot.is_some() {
                runtime.demo_reveal()?;
                runtime.demo_correct()?;
                runtime.demo_repair(&runtime.state().goal, "disambiguation")?;
                if runtime.state().autonomy != "yolo" {
                    runtime.autonomy()?;
                }
                if let Some(path) = snapshot {
                    let events = runtime.store.lock().unwrap().events.clone();
                    capture(
                        &path,
                        width,
                        height,
                        &runtime.state(),
                        &events,
                        runtime.config.color,
                    )?;
                }
                print_summary(&runtime);
                Ok(())
            } else {
                let dir = runtime.store.lock().unwrap().dir.clone();
                tui::run(Some(runtime.clone()), vec![], dir, false).await
            };
            runtime.shutdown().await?;
            result?;
        }
        Commands::Run {
            goal,
            headless,
            provider,
            model,
            constraint,
        } => {
            if headless && goal.is_none() {
                bail!("headless run requires a goal");
            }
            if let Some(alias) = provider {
                config.active_provider = alias;
                config
                    .overrides
                    .insert("active_provider".into(), "CLI --provider".into());
            }
            if let Some(model) = model {
                let p = config
                    .providers
                    .get_mut(&config.active_provider)
                    .context("configure selected provider before --model")?;
                p.model = model;
                config
                    .overrides
                    .insert("providers".into(), "file with CLI --model override".into());
            }
            config.validate()?;
            let runtime = Runtime::new(&cli.data_dir, config, false)?;
            if let Some(goal) = goal {
                runtime.set_goal(&goal, constraint)?;
                runtime.launch()?;
            } else if !constraint.is_empty() {
                runtime.set_goal("Awaiting user task; press i to compose", constraint)?;
            }
            let result = if headless {
                tokio::select! {_=runtime.wait_idle()=>{},_=tokio::signal::ctrl_c()=>{runtime.cancel("headless Ctrl-C")?;runtime.wait_idle().await;}}
                print_summary(&runtime);
                if runtime
                    .store
                    .lock()
                    .unwrap()
                    .events
                    .iter()
                    .any(|e| matches!(&e.kind,Kind::Failure{outcome,..}if outcome=="incomplete"))
                {
                    Err(anyhow::anyhow!(
                        "run incomplete; inspect retained events and artifacts"
                    ))
                } else {
                    Ok(())
                }
            } else {
                let dir = runtime.store.lock().unwrap().dir.clone();
                tui::run(Some(runtime.clone()), vec![], dir, false).await
            };
            runtime.shutdown().await?;
            result?;
        }
        Commands::Exec { executable, args } => {
            let runtime = Runtime::new(&cli.data_dir, config, false)?;
            runtime.set_goal(&format!("Direct user invocation of {executable}"), vec![])?;
            let result = runtime.execute_direct(executable, args).await;
            runtime.shutdown().await?;
            print_summary(&runtime);
            println!("{}", serde_json::to_string_pretty(&result?)?);
        }
    }
    Ok(())
}
fn print_summary(r: &Runtime) {
    let s = r.state();
    println!(
        "{}",
        serde_json::json!({"run_id":r.id(),"journal":r.store.lock().unwrap().dir.join("events.jsonl"),"synthetic":s.demo,"revision":s.revision,"autonomy":s.autonomy,"status":s.status,"receipt":s.receipt,"artifacts":s.artifacts,"answer":s.current_answer})
    );
}
fn capture(
    path: &Path,
    width: u16,
    height: u16,
    state: &State,
    events: &[friendo::model::Event],
    color: bool,
) -> Result<()> {
    if width < 20 || height < 8 || width > 300 || height > 100 {
        bail!("snapshot size must be 20..300 by 8..100");
    }
    let mut terminal = Terminal::new(TestBackend::new(width, height))?;
    let mut view = View::new();
    view.lens = 1;
    view.selected = Some("plan".into());
    view.effects = "off".into();
    view.phase = 2.;
    view.color = color;
    terminal.draw(|f| tui::render(f, state, events, &view))?;
    let buffer = terminal.backend().buffer();
    let mut file = store::private_file(path)?;
    if path.extension().is_some_and(|x| x == "svg") {
        writeln!(
            file,
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}\" height=\"{}\" viewBox=\"0 0 {} {}\"><rect width=\"100%\" height=\"100%\" fill=\"#050911\"/><g font-family=\"Menlo,monospace\" font-size=\"14\">",
            width as usize * 9,
            height as usize * 18,
            width as usize * 9,
            height as usize * 18
        )?;
        for y in 0..height {
            for x in 0..width {
                let cell = &buffer[(x, y)];
                let fg = css_color(cell.fg);
                let bg = if cell.bg == ratatui::style::Color::Reset {
                    "#050911".into()
                } else {
                    css_color(cell.bg)
                };
                let symbol = cell
                    .symbol()
                    .replace('&', "&amp;")
                    .replace('<', "&lt;")
                    .replace('>', "&gt;");
                writeln!(
                    file,
                    "<rect x=\"{}\" y=\"{}\" width=\"9\" height=\"18\" fill=\"{}\"/><text x=\"{}\" y=\"{}\" fill=\"{}\" xml:space=\"preserve\">{}</text>",
                    x as usize * 9,
                    y as usize * 18,
                    bg,
                    x as usize * 9,
                    y as usize * 18 + 14,
                    fg,
                    symbol
                )?;
            }
        }
        writeln!(file, "</g></svg>")?;
    } else {
        for y in 0..height {
            for x in 0..width {
                write!(file, "{}", buffer[(x, y)].symbol())?;
            }
            writeln!(file)?;
        }
    }
    Ok(())
}
fn css_color(color: ratatui::style::Color) -> String {
    use ratatui::style::Color;
    match color {
        Color::Rgb(r, g, b) => format!("#{r:02x}{g:02x}{b:02x}"),
        Color::Black => "#000000".into(),
        Color::White => "#ffffff".into(),
        _ => "#d5e7f0".into(),
    }
}
