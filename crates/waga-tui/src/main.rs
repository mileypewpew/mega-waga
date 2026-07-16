//! WAGA CLI: `waga tick` and `waga pet` (Ratatui companion).

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::{Frame, Terminal};
use std::io::stdout;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use waga_pet::{mood_from_snapshot, sprite, PetMood};
use waga_world::{format_tick_summary, peek_snapshot, run_tick};

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
    /// Advance the park by one tick (headless).
    Tick {
        /// Data directory for world.json + narrative.jsonl
        #[arg(long, default_value = ".waga")]
        data_dir: PathBuf,

        /// Optional persona TOML path
        #[arg(long)]
        persona: Option<PathBuf>,

        /// Optional git repo path (default: discover from cwd)
        #[arg(long)]
        repo: Option<PathBuf>,
    },
    /// Show the Waga pet (Ratatui). Keys: t tick, r refresh, q quit.
    Pet {
        #[arg(long, default_value = ".waga")]
        data_dir: PathBuf,

        #[arg(long)]
        persona: Option<PathBuf>,

        #[arg(long)]
        repo: Option<PathBuf>,

        /// Auto-tick interval in seconds (0 = manual only)
        #[arg(long, default_value_t = 10)]
        every: u64,
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
        } => {
            let result = run_tick(&data_dir, persona.as_deref(), repo.as_deref())
                .context("tick failed")?;
            println!("{}", format_tick_summary(&result));
        }
        Commands::Pet {
            data_dir,
            persona,
            repo,
            every,
        } => {
            run_pet_ui(data_dir, persona, repo, every)?;
        }
    }
    Ok(())
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
    every: Duration,
    last_auto: Instant,
}

impl PetApp {
    fn new(
        data_dir: PathBuf,
        persona: Option<PathBuf>,
        repo: Option<PathBuf>,
        every_secs: u64,
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
            every: Duration::from_secs(every_secs),
            last_auto: Instant::now(),
        };
        // Load last snapshot if present, else tick once so the pet has life.
        match peek_snapshot(&app.data_dir, "strict-cto") {
            Ok(s) if s.tick > 0 => {
                app.apply_snapshot(&s, waga_character::strict_cto_builtin().notice(&s));
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

    fn do_tick(&mut self) -> Result<()> {
        let result = run_tick(
            &self.data_dir,
            self.persona.as_deref(),
            self.repo.as_deref(),
        )?;
        self.apply_snapshot(&result.snapshot, result.notice);
        self.status = format!("ticked → {}", result.pet_mood);
        self.last_auto = Instant::now();
        Ok(())
    }
}

fn run_pet_ui(
    data_dir: PathBuf,
    persona: Option<PathBuf>,
    repo: Option<PathBuf>,
    every: u64,
) -> Result<()> {
    let mut app = PetApp::new(data_dir, persona, repo, every)?;

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

fn pet_loop(terminal: &mut Terminal<impl ratatui::backend::Backend>, app: &mut PetApp) -> Result<()> {
    loop {
        terminal.draw(|f| draw_pet(f, app))?;

        // Auto-tick
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
            Constraint::Min(8),
            Constraint::Length(5),
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
                .title("speech (persona)"),
        );
    f.render_widget(bubble, chunks[2]);

    let footer = Paragraph::new(format!(
        "{}  |  {}  |  keys: t/r tick · q quit",
        app.git_line, app.status
    ))
    .block(Block::default().borders(Borders::ALL).title("sensors"));
    f.render_widget(footer, chunks[3]);

    // Silence unused warning if Rect helpers change
    let _: Rect = chunks[0];
}
