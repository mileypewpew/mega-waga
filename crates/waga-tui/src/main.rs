//! WAGA CLI: tick, events, stories, memories, skills, status, pet, daemon.

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::{Frame, Terminal};
use std::io::stdout;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};
use waga_core::StoryStatus;
use waga_events::{format_event_line, format_story_line, EventLog, StoryStore};
use waga_memory::{
    format_memory_line, format_park_status, format_skill_line, last_memory_line, list_memories,
    list_skills, skills_summary_line,
};
use waga_pet::{mood_from_snapshot, sprite, PetMood};
use waga_media::{control, format_now_playing, now_playing, MediaCommand};
use waga_music::{
    bed_start, bed_steer, bed_stop, direct_from_world, format_music_status, waga_bed_line,
    MusicSession,
};
use waga_voice::{
    example_voice_toml, load_voice_config, resolve_provider, speak, SpeakIntent, VoiceProvider,
};
use waga_world::{
    export_bridge, format_daemon_status, format_inbox_line, format_tick_summary,
    is_interesting_tick, load_inbox_last, notify_entries_for_tick, peek_snapshot, post_inbox,
    run_tick_with, update_watch, DaemonStatus, DaemonWatch, NotifyBus, TickOptions,
};

#[derive(Parser, Debug)]
#[command(
    name = "waga",
    about = "World-Aware General Agent — tick kernel + Waga pet",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Advance the park by one tick (headless; appends to events.jsonl).
    Tick {
        #[arg(long, default_value = ".waga")]
        data_dir: PathBuf,
        #[arg(long)]
        persona: Option<PathBuf>,
        #[arg(long)]
        repo: Option<PathBuf>,
        /// Speak high-signal notify lines (story/XP) via premium TTS.
        #[arg(long, default_value_t = true)]
        voice: bool,
        /// Force silence even if voice.toml is enabled.
        #[arg(long, default_value_t = false)]
        no_voice: bool,
    },
    /// One-screen park snapshot (git, story, memory, skills).
    Status {
        #[arg(long, default_value = ".waga")]
        data_dir: PathBuf,
    },
    /// Speak text with premium TTS (xAI / OpenAI / ElevenLabs).
    Say {
        /// Text to speak
        text: String,
        #[arg(long, default_value = ".waga")]
        data_dir: PathBuf,
        /// Provider override: auto | xai | openai | elevenlabs | null
        #[arg(long)]
        provider: Option<String>,
        /// Write mp3 but do not play
        #[arg(long, default_value_t = false)]
        no_play: bool,
    },
    /// Print example voice.toml and where to put it.
    VoiceConfig,
    /// Show the Waga pet (Ratatui). Keys: t tick, r refresh, q quit.
    Pet {
        #[arg(long, default_value = ".waga")]
        data_dir: PathBuf,
        #[arg(long)]
        persona: Option<PathBuf>,
        #[arg(long)]
        repo: Option<PathBuf>,
        #[arg(long, default_value_t = 10)]
        every: u64,
        /// Speak notify lines on tick (default true if keys present).
        #[arg(long, default_value_t = true)]
        voice: bool,
        #[arg(long, default_value_t = false)]
        no_voice: bool,
    },
    /// List recent events from the append-only log.
    Events {
        #[arg(long, default_value = ".waga")]
        data_dir: PathBuf,
        #[arg(long, default_value_t = 20)]
        last: usize,
    },
    /// List stories (open and closed arcs over events).
    Stories {
        #[arg(long, default_value = ".waga")]
        data_dir: PathBuf,
    },
    /// List classified park memories.
    Memories {
        #[arg(long, default_value = ".waga")]
        data_dir: PathBuf,
        #[arg(long, default_value_t = 20)]
        last: usize,
    },
    /// Show park skill XP sheet.
    Skills {
        #[arg(long, default_value = ".waga")]
        data_dir: PathBuf,
    },
    /// Show what's currently playing (MPRIS / playerctl).
    Now,
    /// Control system media or Waga HumanMusic bed.
    Music {
        #[command(subcommand)]
        action: MusicAction,
    },
    /// Always-on park daemon: tick on an interval, notify bus + status file.
    Daemon {
        /// Seconds between ticks.
        #[arg(long, default_value_t = 30)]
        every: u64,
        #[arg(long, default_value = ".waga")]
        data_dir: PathBuf,
        #[arg(long)]
        persona: Option<PathBuf>,
        #[arg(long)]
        repo: Option<PathBuf>,
        /// Speak high-signal notify lines (default on).
        #[arg(long, default_value_t = true)]
        voice: bool,
        #[arg(long, default_value_t = false)]
        no_voice: bool,
        /// Only print high-signal ticks (quiet heartbeats).
        #[arg(long, default_value_t = false)]
        quiet: bool,
        /// Wait `every` before the first tick (default ticks immediately).
        #[arg(long, default_value_t = false)]
        no_immediate: bool,
    },
    /// Print always-on daemon status (`.waga/daemon.json`).
    DaemonStatus {
        #[arg(long, default_value = ".waga")]
        data_dir: PathBuf,
    },
    /// Cooperative stop request (sets running=false; loop also exits on Ctrl+C).
    DaemonStop {
        #[arg(long, default_value = ".waga")]
        data_dir: PathBuf,
    },
    /// Tail the file-backed notify bus (`.waga/notify.jsonl`).
    Notifies {
        #[arg(long, default_value = ".waga")]
        data_dir: PathBuf,
        #[arg(long, default_value_t = 20)]
        last: usize,
    },
    /// Grok Build file bridge (world export + inbox).
    Bridge {
        #[command(subcommand)]
        action: BridgeAction,
    },
}

#[derive(Subcommand, Debug)]
enum BridgeAction {
    /// Write `.waga/bridge/world.md` + `world.json` for Grok Build to read.
    Export {
        #[arg(long, default_value = ".waga")]
        data_dir: PathBuf,
    },
    /// Show export paths + short blurb + recent inbox.
    Status {
        #[arg(long, default_value = ".waga")]
        data_dir: PathBuf,
    },
    /// List messages Build (or `bridge post`) wrote to the inbox.
    Inbox {
        #[arg(long, default_value = ".waga")]
        data_dir: PathBuf,
        #[arg(long, default_value_t = 20)]
        last: usize,
    },
    /// Post a message into the park inbox (simulates Grok Build).
    Post {
        /// Message body
        text: String,
        #[arg(long, default_value = "note")]
        kind: String,
        #[arg(long, default_value = "grok-build")]
        source: String,
        #[arg(long)]
        session: Option<String>,
        #[arg(long, default_value = ".waga")]
        data_dir: PathBuf,
    },
}

#[derive(Subcommand, Debug)]
enum MusicAction {
    /// Resume playback (MPRIS).
    Play,
    /// Pause (MPRIS).
    Pause,
    /// Toggle play/pause (MPRIS).
    Toggle,
    /// Next track (MPRIS).
    Next,
    /// Previous track (MPRIS).
    Prev,
    /// Stop (MPRIS).
    Stop,
    /// HumanMusic SuperCollider live bed.
    Bed {
        #[command(subcommand)]
        cmd: BedAction,
    },
}

#[derive(Subcommand, Debug)]
enum BedAction {
    /// Start bed (OSC + optional sclang).
    Start {
        #[arg(long, default_value = ".waga")]
        data_dir: PathBuf,
    },
    /// Soft-stop bed (gate off).
    Stop {
        #[arg(long, default_value = ".waga")]
        data_dir: PathBuf,
    },
    /// Re-steer from current world snapshot.
    Steer {
        #[arg(long, default_value = ".waga")]
        data_dir: PathBuf,
    },
    /// Show bed session status.
    Status {
        #[arg(long, default_value = ".waga")]
        data_dir: PathBuf,
    },
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_target(false)
        .init();

    let cli = Cli::parse();
    match cli.command {
        Commands::Tick {
            data_dir,
            persona,
            repo,
            voice,
            no_voice,
        } => {
            let voice_on = voice && !no_voice;
            let result = run_tick_with(
                &data_dir,
                persona.as_deref(),
                repo.as_deref(),
                TickOptions { voice: voice_on },
            )
            .context("tick failed")?;
            // Refresh file bridge for Grok Build after each manual tick.
            if let Err(e) = export_bridge(&data_dir) {
                tracing::warn!("bridge export: {e}");
            }
            println!("{}", format_tick_summary(&result));
        }
        Commands::Say {
            text,
            data_dir,
            provider,
            no_play,
        } => {
            let mut cfg = load_voice_config(Some(&data_dir));
            if no_play {
                cfg.play = false;
            }
            if let Some(p) = provider {
                cfg.default_provider = parse_provider(&p).context("provider")?;
            }
            let path = speak(&cfg, &text, SpeakIntent::Explicit, &data_dir)
                .map_err(|e| anyhow::anyhow!("{e}"))?;
            let resolved = resolve_provider(&cfg, SpeakIntent::Explicit)
                .map(|p| format!("{p:?}"))
                .unwrap_or_else(|_| "?".into());
            println!("spoke via {resolved} → {}", path.display());
        }
        Commands::VoiceConfig => {
            println!("# Write to .waga/voice.toml or ~/.config/waga/voice.toml\n");
            println!("{}", example_voice_toml());
            println!("# Env keys: XAI_API_KEY, OPENAI_API_KEY, ELEVENLABS_API_KEY");
        }
        Commands::Status { data_dir } => {
            let snap = peek_snapshot(&data_dir, "strict-cto").context("peek world")?;
            let store = StoryStore::load(&data_dir).context("stories")?;
            let open = store
                .stories
                .iter()
                .find(|s| s.status == StoryStatus::Open)
                .map(|s| s.title.as_str());
            println!(
                "{}",
                format_park_status(&data_dir, &snap, open).context("status")?
            );
        }
        Commands::Pet {
            data_dir,
            persona,
            repo,
            every,
            voice,
            no_voice,
        } => {
            run_pet_ui(data_dir, persona, repo, every, voice && !no_voice)?;
        }
        Commands::Events { data_dir, last } => {
            let log = EventLog::open(&data_dir).context("open event log")?;
            let events = log.load_all().context("load events")?;
            let start = events.len().saturating_sub(last);
            for e in &events[start..] {
                println!("{}", format_event_line(e));
            }
            if events.is_empty() {
                println!("(no events yet — run `waga tick`)");
            }
        }
        Commands::Stories { data_dir } => {
            let store = StoryStore::load(&data_dir).context("load stories")?;
            if store.stories.is_empty() {
                println!("(no stories yet — dirty git + tick opens one)");
            } else {
                for s in &store.stories {
                    println!("{}", format_story_line(s));
                }
            }
        }
        Commands::Memories { data_dir, last } => {
            let mems = list_memories(&data_dir).context("load memories")?;
            if mems.is_empty() {
                println!("(no memories yet — close a git story with a clean tree)");
            } else {
                let start = mems.len().saturating_sub(last);
                for m in &mems[start..] {
                    println!("{}", format_memory_line(m));
                }
            }
        }
        Commands::Skills { data_dir } => {
            let skills = list_skills(&data_dir).context("load skills")?;
            if skills.is_empty() {
                println!("(no XP yet — park skill sheet is empty)");
            } else {
                for s in skills {
                    println!("{}", format_skill_line(&s));
                }
            }
        }
        Commands::Now => {
            let np = now_playing();
            println!("{}", format_now_playing(&np));
            if let Ok(session) = MusicSession::load(PathBuf::from(".waga").as_path()) {
                println!();
                println!("{}", waga_bed_line(&session));
            }
        }
        Commands::Music { action } => match action {
            MusicAction::Play => {
                control(MediaCommand::Play).map_err(|e| anyhow::anyhow!("{e}"))?;
                println!("{}", format_now_playing(&now_playing()));
            }
            MusicAction::Pause => {
                control(MediaCommand::Pause).map_err(|e| anyhow::anyhow!("{e}"))?;
                println!("{}", format_now_playing(&now_playing()));
            }
            MusicAction::Toggle => {
                control(MediaCommand::PlayPause).map_err(|e| anyhow::anyhow!("{e}"))?;
                println!("{}", format_now_playing(&now_playing()));
            }
            MusicAction::Next => {
                control(MediaCommand::Next).map_err(|e| anyhow::anyhow!("{e}"))?;
                println!("{}", format_now_playing(&now_playing()));
            }
            MusicAction::Prev => {
                control(MediaCommand::Previous).map_err(|e| anyhow::anyhow!("{e}"))?;
                println!("{}", format_now_playing(&now_playing()));
            }
            MusicAction::Stop => {
                control(MediaCommand::Stop).map_err(|e| anyhow::anyhow!("{e}"))?;
                println!("{}", format_now_playing(&now_playing()));
            }
            MusicAction::Bed { cmd } => match cmd {
                BedAction::Start { data_dir } => {
                    // Ensure SC script is reachable from data dir
                    let src = PathBuf::from("assets/sc/waga_bed.scd");
                    let dst = data_dir.join("waga_bed.scd");
                    if src.is_file() && !dst.is_file() {
                        let _ = std::fs::create_dir_all(&data_dir);
                        let _ = std::fs::copy(&src, &dst);
                    }
                    let snap = peek_snapshot(&data_dir, "strict-cto")?;
                    let params = direct_from_world(&snap, &[]);
                    let session =
                        bed_start(&data_dir, params).map_err(|e| anyhow::anyhow!("{e}"))?;
                    println!("{}", format_music_status(&session));
                }
                BedAction::Stop { data_dir } => {
                    let session = bed_stop(&data_dir).map_err(|e| anyhow::anyhow!("{e}"))?;
                    println!("{}", format_music_status(&session));
                }
                BedAction::Steer { data_dir } => {
                    let snap = peek_snapshot(&data_dir, "strict-cto")?;
                    let params = direct_from_world(&snap, &[]);
                    let session =
                        bed_steer(&data_dir, params).map_err(|e| anyhow::anyhow!("{e}"))?;
                    println!("{}", format_music_status(&session));
                }
                BedAction::Status { data_dir } => {
                    let session = MusicSession::load(&data_dir)?;
                    println!("{}", format_music_status(&session));
                }
            },
        },
        Commands::Daemon {
            every,
            data_dir,
            persona,
            repo,
            voice,
            no_voice,
            quiet,
            no_immediate,
        } => {
            run_daemon(
                data_dir,
                persona,
                repo,
                every,
                voice && !no_voice,
                quiet,
                !no_immediate,
            )?;
        }
        Commands::DaemonStatus { data_dir } => {
            match DaemonStatus::load(&data_dir).context("load daemon status")? {
                Some(s) => println!("{}", format_daemon_status(&s)),
                None => println!(
                    "(no daemon status — run `waga daemon` first; expected {})",
                    data_dir.join("daemon.json").display()
                ),
            }
        }
        Commands::DaemonStop { data_dir } => {
            match DaemonStatus::load(&data_dir).context("load daemon status")? {
                Some(mut s) if s.running => {
                    s.mark_stopped(&data_dir)?;
                    println!(
                        "stop requested for pid={} (daemon exits within one interval)",
                        s.pid
                    );
                }
                Some(s) => println!("daemon already stopped (pid={})", s.pid),
                None => println!("(no daemon status file — nothing to stop)"),
            }
        }
        Commands::Notifies { data_dir, last } => {
            let bus = NotifyBus::open(&data_dir);
            let entries = bus.load_last(last).context("load notify bus")?;
            if entries.is_empty() {
                println!("(no notify bus entries yet — high-signal daemon ticks write here)");
            } else {
                for e in entries {
                    println!(
                        "tick={} [{}] {} · mood={}",
                        e.tick, e.kind, e.text, e.mood
                    );
                }
            }
        }
        Commands::Bridge { action } => match action {
            BridgeAction::Export { data_dir } => {
                let w = export_bridge(&data_dir).context("bridge export")?;
                println!(
                    "exported tick={} → {} and {}",
                    w.tick,
                    data_dir.join("bridge/world.md").display(),
                    data_dir.join("bridge/world.json").display()
                );
                println!("{}", w.blurb);
            }
            BridgeAction::Status { data_dir } => {
                let w = export_bridge(&data_dir).context("bridge export")?;
                println!("bridge dir: {}", data_dir.join("bridge").display());
                println!("world.md / world.json updated at {}", w.exported_at);
                println!();
                println!("{}", w.blurb);
                println!();
                let inbox = load_inbox_last(&data_dir, 5).context("inbox")?;
                if inbox.is_empty() {
                    println!("inbox: (empty — Build can append bridge/inbox.jsonl)");
                } else {
                    println!("inbox (last {}):", inbox.len());
                    for m in inbox {
                        println!("  {}", format_inbox_line(&m));
                    }
                }
            }
            BridgeAction::Inbox { data_dir, last } => {
                let inbox = load_inbox_last(&data_dir, last).context("inbox")?;
                if inbox.is_empty() {
                    println!("(inbox empty — use `waga bridge post` or append inbox.jsonl)");
                } else {
                    for m in inbox {
                        println!("{}", format_inbox_line(&m));
                    }
                }
            }
            BridgeAction::Post {
                text,
                kind,
                source,
                session,
                data_dir,
            } => {
                let msg = post_inbox(&data_dir, kind, text, source, session)
                    .context("append inbox")?;
                // Refresh export so Build status is visible in world.md
                let _ = export_bridge(&data_dir);
                println!("posted: {}", format_inbox_line(&msg));
            }
        },
    }
    Ok(())
}

fn open_story_title(data_dir: &Path) -> Result<Option<String>> {
    let store = StoryStore::load(data_dir)?;
    Ok(store
        .stories
        .iter()
        .find(|s| s.status == StoryStatus::Open)
        .map(|s| s.title.clone()))
}

fn run_daemon(
    data_dir: PathBuf,
    persona: Option<PathBuf>,
    repo: Option<PathBuf>,
    every_secs: u64,
    voice: bool,
    quiet: bool,
    immediate: bool,
) -> Result<()> {
    if every_secs == 0 {
        anyhow::bail!("--every must be >= 1 second");
    }

    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();
    ctrlc::set_handler(move || {
        r.store(false, Ordering::SeqCst);
        eprintln!("\n[waga daemon] Ctrl+C — shutting down…");
    })
    .context("install Ctrl+C handler")?;

    let mut status = DaemonStatus::start(every_secs);
    status.save(&data_dir)?;
    let bus = NotifyBus::open(&data_dir);

    let snap = peek_snapshot(&data_dir, "strict-cto").unwrap_or_else(|_| {
        waga_core::WorldSnapshot::fresh("strict-cto")
    });
    let mut watch = DaemonWatch {
        last_mood: mood_from_snapshot(&snap).as_str().into(),
        open_story_title: open_story_title(&data_dir).ok().flatten(),
    };

    println!(
        "waga daemon started pid={} every={}s voice={} quiet={} data={}",
        status.pid,
        every_secs,
        if voice { "on" } else { "off" },
        if quiet { "yes" } else { "no" },
        data_dir.display()
    );
    println!("Ctrl+C or `waga daemon-stop` to exit.\n");

    let mut first = true;
    while running.load(Ordering::SeqCst) {
        // Cooperative stop via `waga daemon-stop` (do not clobber with later saves)
        if stop_requested(&data_dir, status.pid) {
            println!("[waga daemon] stop flag set — exiting");
            break;
        }

        if first && !immediate {
            first = false;
        } else {
            first = false;
            match run_tick_with(
                &data_dir,
                persona.as_deref(),
                repo.as_deref(),
                TickOptions { voice },
            ) {
                Ok(result) => {
                    let open = open_story_title(&data_dir).ok().flatten();
                    let interesting =
                        is_interesting_tick(&watch, &result, open.as_deref());

                    if interesting {
                        for entry in notify_entries_for_tick(&watch, &result, open.as_deref()) {
                            if let Err(e) = bus.append(&entry) {
                                tracing::warn!("notify bus: {e}");
                            }
                        }
                    }

                    update_watch(&mut watch, &result, open);
                    status.record_tick(&result, interesting);
                    // Keep Grok Build bridge files fresh (world blurb).
                    if let Err(e) = export_bridge(&data_dir) {
                        tracing::warn!("bridge export: {e}");
                    }
                    // If stop was requested during the tick, keep running=false on disk.
                    if stop_requested(&data_dir, status.pid) {
                        if interesting || !quiet {
                            let tag = if interesting { "!" } else { "." };
                            println!("[{tag}] {}", format_tick_summary(&result));
                        }
                        println!("[waga daemon] stop flag set during tick — exiting");
                        break;
                    }
                    if let Err(e) = status.save(&data_dir) {
                        tracing::warn!("daemon status: {e}");
                    }

                    if interesting || !quiet {
                        let tag = if interesting { "!" } else { "." };
                        println!("[{tag}] {}", format_tick_summary(&result));
                    } else {
                        tracing::debug!(
                            tick = result.snapshot.tick,
                            mood = %result.pet_mood,
                            "quiet heartbeat"
                        );
                    }
                }
                Err(e) => {
                    eprintln!("[waga daemon] tick error: {e}");
                    tracing::error!("daemon tick failed: {e}");
                }
            }
        }

        if stop_requested(&data_dir, status.pid) || !running.load(Ordering::SeqCst) {
            break;
        }

        // Sleep in slices so Ctrl+C / stop flag are responsive
        let slice = Duration::from_millis(200);
        let deadline = Instant::now() + Duration::from_secs(every_secs);
        while Instant::now() < deadline && running.load(Ordering::SeqCst) {
            if stop_requested(&data_dir, status.pid) {
                break;
            }
            thread::sleep(slice);
        }
    }

    status.mark_stopped(&data_dir)?;
    println!(
        "waga daemon stopped after {} ticks ({} interesting)",
        status.ticks_total, status.interesting_total
    );
    Ok(())
}

/// True when `daemon-stop` cleared the running flag for this process.
fn stop_requested(data_dir: &Path, pid: u32) -> bool {
    match DaemonStatus::load(data_dir) {
        Ok(Some(s)) => !s.running && s.pid == pid,
        _ => false,
    }
}

struct PetApp {
    data_dir: PathBuf,
    persona: Option<PathBuf>,
    repo: Option<PathBuf>,
    notice: String,
    mood: PetMood,
    tick: u64,
    git_line: String,
    status: String,
    memory_line: String,
    skills_line: String,
    story_line: String,
    now_line: String,
    bed_line: String,
    every: Duration,
    last_auto: Instant,
    voice: bool,
}

impl PetApp {
    fn new(
        data_dir: PathBuf,
        persona: Option<PathBuf>,
        repo: Option<PathBuf>,
        every_secs: u64,
        voice: bool,
    ) -> Result<Self> {
        let mut app = Self {
            data_dir,
            persona,
            repo,
            notice: "Press t to tick, q to quit.".into(),
            mood: PetMood::Idle,
            tick: 0,
            git_line: "git: (none)".into(),
            status: String::new(),
            memory_line: String::new(),
            skills_line: String::new(),
            story_line: "story: —".into(),
            now_line: String::new(),
            bed_line: String::new(),
            every: Duration::from_secs(every_secs),
            last_auto: Instant::now(),
            voice,
        };
        match peek_snapshot(&app.data_dir, "strict-cto") {
            Ok(s) if s.tick > 0 => {
                let notice = waga_character::strict_cto_builtin().notice(&s);
                app.apply_snapshot(&s, notice);
                app.refresh_growth()?;
            }
            _ => {
                app.do_tick()?;
            }
        }
        Ok(app)
    }

    fn apply_snapshot(&mut self, s: &waga_core::WorldSnapshot, notice: String) {
        self.tick = s.tick;
        self.mood = mood_from_snapshot(s);
        self.notice = notice;
        self.git_line = match &s.git {
            Some(g) => format!(
                "{} @ {} — {}",
                g.branch,
                g.repo_path.display(),
                if g.dirty { "DIRTY" } else { "clean" }
            ),
            None => "git: (none)".into(),
        };
    }

    fn refresh_growth(&mut self) -> Result<()> {
        self.memory_line = last_memory_line(&self.data_dir).unwrap_or_else(|_| "memory: ?".into());
        self.skills_line =
            skills_summary_line(&self.data_dir).unwrap_or_else(|_| "skills: ?".into());
        let store = StoryStore::load(&self.data_dir)?;
        self.story_line = store
            .stories
            .iter()
            .find(|s| s.status == StoryStatus::Open)
            .map(|s| format!("story: OPEN \"{}\"", s.title))
            .unwrap_or_else(|| "story: none open".into());
        self.now_line = now_playing().display_line();
        self.bed_line = MusicSession::load(&self.data_dir)
            .map(|s| waga_bed_line(&s))
            .unwrap_or_else(|_| "♫ bed: ?".into());
        Ok(())
    }

    fn do_tick(&mut self) -> Result<()> {
        let result = run_tick_with(
            &self.data_dir,
            self.persona.as_deref(),
            self.repo.as_deref(),
            TickOptions { voice: self.voice },
        )?;
        self.apply_snapshot(&result.snapshot, result.notice);
        self.status = format!(
            "ticked → {} · mem+{} xp+{} · voice={}",
            result.pet_mood,
            result.memories_formed,
            result.xp_granted,
            if self.voice { "on" } else { "off" }
        );
        self.refresh_growth()?;
        self.last_auto = Instant::now();
        Ok(())
    }
}

fn parse_provider(s: &str) -> Result<VoiceProvider> {
    match s.to_ascii_lowercase().as_str() {
        "auto" => Ok(VoiceProvider::Auto),
        "xai" | "grok" => Ok(VoiceProvider::Xai),
        "openai" | "oai" => Ok(VoiceProvider::Openai),
        "elevenlabs" | "el" | "11" => Ok(VoiceProvider::Elevenlabs),
        "null" | "off" | "none" => Ok(VoiceProvider::Null),
        other => anyhow::bail!("unknown provider '{other}' (auto|xai|openai|elevenlabs|null)"),
    }
}

fn run_pet_ui(
    data_dir: PathBuf,
    persona: Option<PathBuf>,
    repo: Option<PathBuf>,
    every: u64,
    voice: bool,
) -> Result<()> {
    let mut app = PetApp::new(data_dir, persona, repo, every, voice)?;

    enable_raw_mode()?;
    let mut out = stdout();
    execute!(out, EnterAlternateScreen)?;
    let backend = ratatui::backend::CrosstermBackend::new(out);
    let mut terminal = Terminal::new(backend)?;

    let res = pet_loop(&mut terminal, &mut app);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    res
}

fn pet_loop(
    terminal: &mut Terminal<impl ratatui::backend::Backend>,
    app: &mut PetApp,
) -> Result<()> {
    loop {
        terminal.draw(|f| draw_pet(f, app))?;

        if !app.every.is_zero() && app.last_auto.elapsed() >= app.every {
            if let Err(e) = app.do_tick() {
                app.status = format!("auto-tick error: {e}");
            }
        }

        let timeout = if app.every.is_zero() {
            Duration::from_millis(250)
        } else {
            Duration::from_millis(100)
        };

        if event::poll(timeout)? {
            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => break,
                    KeyCode::Char('t') | KeyCode::Char('r') => {
                        if let Err(e) = app.do_tick() {
                            app.status = format!("tick error: {e}");
                        }
                    }
                    KeyCode::Char(' ') => {
                        match control(MediaCommand::PlayPause) {
                            Ok(()) => {
                                app.now_line = now_playing().display_line();
                                app.status = "media toggle".into();
                            }
                            Err(e) => app.status = format!("media: {e}"),
                        }
                    }
                    KeyCode::Char('n') => {
                        match control(MediaCommand::Next) {
                            Ok(()) => {
                                app.now_line = now_playing().display_line();
                                app.status = "media next".into();
                            }
                            Err(e) => app.status = format!("media: {e}"),
                        }
                    }
                    KeyCode::Char('p') => {
                        match control(MediaCommand::Previous) {
                            Ok(()) => {
                                app.now_line = now_playing().display_line();
                                app.status = "media prev".into();
                            }
                            Err(e) => app.status = format!("media: {e}"),
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    Ok(())
}

fn draw_pet(f: &mut Frame, app: &PetApp) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(6),
            Constraint::Length(4),
            Constraint::Length(4),
            Constraint::Length(4),
            Constraint::Length(3),
        ])
        .split(f.area());

    let title = Paragraph::new(Line::from(vec![
        Span::styled(
            " WAGA PET ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(format!(
            " tick {} · mood {} · auto {}s ",
            app.tick,
            app.mood,
            app.every.as_secs()
        )),
    ]))
    .block(Block::default().borders(Borders::ALL).title("world-aware"));
    f.render_widget(title, chunks[0]);

    let mood_color = match app.mood {
        PetMood::Grumpy => Color::Red,
        PetMood::Content => Color::Green,
        PetMood::Idle => Color::Yellow,
    };
    let pet = Paragraph::new(sprite(app.mood))
        .style(Style::default().fg(mood_color))
        .block(Block::default().borders(Borders::ALL).title("waga pet"));
    f.render_widget(pet, chunks[1]);

    let bubble = Paragraph::new(app.notice.as_str())
        .wrap(Wrap { trim: true })
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("speech (persona + memory)"),
        );
    f.render_widget(bubble, chunks[2]);

    let growth = Paragraph::new(format!(
        "{}\n{}\n{}",
        app.story_line, app.memory_line, app.skills_line
    ))
    .wrap(Wrap { trim: true })
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title("growth (park)"),
    );
    f.render_widget(growth, chunks[3]);

    let media = Paragraph::new(format!("{}\n{}", app.now_line, app.bed_line))
        .wrap(Wrap { trim: true })
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("media · HumanMusic"),
        );
    f.render_widget(media, chunks[4]);

    let footer = Paragraph::new(format!(
        "{}  |  {}  |  keys: t tick · space play/pause · n/p track · q quit",
        app.git_line, app.status
    ))
    .block(Block::default().borders(Borders::ALL).title("sensors"));
    f.render_widget(footer, chunks[5]);
}
