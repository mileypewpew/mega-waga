//! Premium multi-provider TTS for park notifications.
//!
//! Providers: xAI Grok TTS, OpenAI TTS, ElevenLabs.
//! Notify-first (story / XP / manual say). Realtime duplex is later.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};
use waga_core::{Event, EventBody};

/// Voice subsystem errors.
#[derive(Debug, thiserror::Error)]
pub enum VoiceError {
    #[error("I/O: {0}")]
    Io(#[from] std::io::Error),
    #[error("config: {0}")]
    Config(String),
    #[error("HTTP: {0}")]
    Http(String),
    #[error("no TTS provider available (set XAI_API_KEY, OPENAI_API_KEY, or ELEVENLABS_API_KEY)")]
    NoProvider,
    #[error("playback failed: {0}")]
    Playback(String),
    #[error("{0}")]
    Msg(String),
}

pub type Result<T> = std::result::Result<T, VoiceError>;

/// Which cloud synthesizer to use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum VoiceProvider {
    #[default]
    Auto,
    Xai,
    Openai,
    Elevenlabs,
    /// Silent / tests
    Null,
}

/// Intent biases auto-routing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpeakIntent {
    /// Character / pet line (prefer ElevenLabs when available).
    Pet,
    /// Fast utility line (prefer OpenAI).
    Fast,
    /// Default park notify (prefer xAI).
    Default,
    /// Explicit provider from CLI.
    Explicit,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub default_provider: VoiceProvider,
    /// Play audio after synthesize (otherwise write cache only).
    #[serde(default = "default_true")]
    pub play: bool,
    /// Directory for last utterance mp3 (under data dir if relative).
    #[serde(default = "default_cache")]
    pub cache_dir: String,
    #[serde(default)]
    pub xai: XaiConfig,
    #[serde(default)]
    pub openai: OpenaiConfig,
    #[serde(default)]
    pub elevenlabs: ElevenlabsConfig,
    #[serde(default)]
    pub auto: AutoRoutes,
}

fn default_true() -> bool {
    true
}
fn default_cache() -> String {
    "voice_cache".into()
}

impl Default for VoiceConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            default_provider: VoiceProvider::Auto,
            play: true,
            cache_dir: default_cache(),
            xai: XaiConfig::default(),
            openai: OpenaiConfig::default(),
            elevenlabs: ElevenlabsConfig::default(),
            auto: AutoRoutes::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct XaiConfig {
    #[serde(default = "xai_key_env")]
    pub api_key_env: String,
    #[serde(default = "xai_voice")]
    pub voice_id: String,
    #[serde(default = "lang_en")]
    pub language: String,
}

fn xai_key_env() -> String {
    "XAI_API_KEY".into()
}
fn xai_voice() -> String {
    "eve".into()
}
fn lang_en() -> String {
    "en".into()
}

impl Default for XaiConfig {
    fn default() -> Self {
        Self {
            api_key_env: xai_key_env(),
            voice_id: xai_voice(),
            language: lang_en(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenaiConfig {
    #[serde(default = "openai_key_env")]
    pub api_key_env: String,
    #[serde(default = "openai_model")]
    pub model: String,
    #[serde(default = "openai_voice")]
    pub voice: String,
}

fn openai_key_env() -> String {
    "OPENAI_API_KEY".into()
}
fn openai_model() -> String {
    "tts-1-hd".into()
}
fn openai_voice() -> String {
    "nova".into()
}

impl Default for OpenaiConfig {
    fn default() -> Self {
        Self {
            api_key_env: openai_key_env(),
            model: openai_model(),
            voice: openai_voice(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ElevenlabsConfig {
    #[serde(default = "el_key_env")]
    pub api_key_env: String,
    /// Required for ElevenLabs (no good universal default).
    #[serde(default)]
    pub voice_id: String,
    #[serde(default = "el_model")]
    pub model_id: String,
}

fn el_key_env() -> String {
    "ELEVENLABS_API_KEY".into()
}
fn el_model() -> String {
    "eleven_multilingual_v2".into()
}

impl Default for ElevenlabsConfig {
    fn default() -> Self {
        Self {
            api_key_env: el_key_env(),
            voice_id: String::new(),
            model_id: el_model(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoRoutes {
    #[serde(default = "route_el")]
    pub notify_pet: VoiceProvider,
    #[serde(default = "route_oai")]
    pub notify_fast: VoiceProvider,
    #[serde(default = "route_xai")]
    pub notify_default: VoiceProvider,
    #[serde(default = "default_fallback")]
    pub fallback: Vec<VoiceProvider>,
}

fn route_el() -> VoiceProvider {
    VoiceProvider::Elevenlabs
}
fn route_oai() -> VoiceProvider {
    VoiceProvider::Openai
}
fn route_xai() -> VoiceProvider {
    VoiceProvider::Xai
}
fn default_fallback() -> Vec<VoiceProvider> {
    vec![
        VoiceProvider::Xai,
        VoiceProvider::Openai,
        VoiceProvider::Elevenlabs,
    ]
}

impl Default for AutoRoutes {
    fn default() -> Self {
        Self {
            notify_pet: route_el(),
            notify_fast: route_oai(),
            notify_default: route_xai(),
            fallback: default_fallback(),
        }
    }
}

/// Load voice config from data dir, then ~/.config/waga/voice.toml, else default.
pub fn load_voice_config(data_dir: Option<&Path>) -> VoiceConfig {
    let candidates: Vec<PathBuf> = [
        data_dir.map(|d| d.join("voice.toml")),
        dirs::config_dir().map(|c| c.join("waga/voice.toml")),
    ]
    .into_iter()
    .flatten()
    .collect();

    for path in candidates {
        if path.is_file() {
            match fs::read_to_string(&path) {
                Ok(text) => match toml::from_str::<VoiceConfig>(&text) {
                    Ok(cfg) => {
                        tracing::info!("loaded voice config from {}", path.display());
                        return cfg;
                    }
                    Err(e) => tracing::warn!("bad voice config {}: {e}", path.display()),
                },
                Err(e) => tracing::warn!("read {}: {e}", path.display()),
            }
        }
    }
    VoiceConfig::default()
}

fn env_key(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|s| !s.is_empty())
}

fn provider_available(cfg: &VoiceConfig, p: VoiceProvider) -> bool {
    match p {
        VoiceProvider::Null => true,
        VoiceProvider::Auto => false,
        VoiceProvider::Xai => env_key(&cfg.xai.api_key_env).is_some(),
        VoiceProvider::Openai => env_key(&cfg.openai.api_key_env).is_some(),
        VoiceProvider::Elevenlabs => {
            env_key(&cfg.elevenlabs.api_key_env).is_some()
                && !cfg.elevenlabs.voice_id.trim().is_empty()
        }
    }
}

/// Resolve concrete provider from intent + config + available keys.
pub fn resolve_provider(cfg: &VoiceConfig, intent: SpeakIntent) -> Result<VoiceProvider> {
    if matches!(cfg.default_provider, VoiceProvider::Null) {
        return Ok(VoiceProvider::Null);
    }

    let preferred = match (cfg.default_provider, intent) {
        (VoiceProvider::Auto, SpeakIntent::Pet) => cfg.auto.notify_pet,
        (VoiceProvider::Auto, SpeakIntent::Fast) => cfg.auto.notify_fast,
        (VoiceProvider::Auto, SpeakIntent::Default | SpeakIntent::Explicit) => {
            cfg.auto.notify_default
        }
        (other, _) => other,
    };

    let mut chain = vec![preferred];
    for f in &cfg.auto.fallback {
        if !chain.contains(f) {
            chain.push(*f);
        }
    }

    for p in chain {
        if provider_available(cfg, p) {
            return Ok(p);
        }
    }

    // Any available
    for p in [
        VoiceProvider::Xai,
        VoiceProvider::Openai,
        VoiceProvider::Elevenlabs,
        VoiceProvider::Null,
    ] {
        if provider_available(cfg, p) {
            return Ok(p);
        }
    }
    Err(VoiceError::NoProvider)
}

/// Synthesize + optional play. Returns path to cached audio.
pub fn speak(
    cfg: &VoiceConfig,
    text: &str,
    intent: SpeakIntent,
    data_dir: &Path,
) -> Result<PathBuf> {
    let text = text.trim();
    if text.is_empty() {
        return Err(VoiceError::Msg("empty speech text".into()));
    }
    if !cfg.enabled {
        return Err(VoiceError::Msg("voice disabled in config".into()));
    }

    let provider = resolve_provider(cfg, intent)?;
    let audio = synthesize(cfg, provider, text)?;
    let cache = data_dir.join(&cfg.cache_dir);
    fs::create_dir_all(&cache)?;
    let path = cache.join(format!(
        "utterance_{}.mp3",
        Instant::now().elapsed().as_nanos()
    ));
    // stable name for "last"
    let last = cache.join("last.mp3");
    fs::write(&path, &audio)?;
    fs::write(&last, &audio)?;

    if cfg.play && !matches!(provider, VoiceProvider::Null) {
        play_mp3(&last)?;
    }
    tracing::info!(?provider, bytes = audio.len(), "spoke: {text}");
    Ok(last)
}

fn synthesize(cfg: &VoiceConfig, provider: VoiceProvider, text: &str) -> Result<Vec<u8>> {
    match provider {
        VoiceProvider::Null => Ok(Vec::new()),
        VoiceProvider::Xai => synth_xai(cfg, text),
        VoiceProvider::Openai => synth_openai(cfg, text),
        VoiceProvider::Elevenlabs => synth_elevenlabs(cfg, text),
        VoiceProvider::Auto => Err(VoiceError::Msg("auto must be resolved first".into())),
    }
}

fn synth_xai(cfg: &VoiceConfig, text: &str) -> Result<Vec<u8>> {
    let key = env_key(&cfg.xai.api_key_env).ok_or(VoiceError::NoProvider)?;
    let body = serde_json::json!({
        "text": text,
        "voice_id": cfg.xai.voice_id,
        "language": cfg.xai.language,
    });
    post_bytes(
        "https://api.x.ai/v1/tts",
        &[
            ("Authorization", &format!("Bearer {key}")),
            ("Content-Type", "application/json"),
        ],
        &body,
    )
}

fn synth_openai(cfg: &VoiceConfig, text: &str) -> Result<Vec<u8>> {
    let key = env_key(&cfg.openai.api_key_env).ok_or(VoiceError::NoProvider)?;
    let body = serde_json::json!({
        "model": cfg.openai.model,
        "input": text,
        "voice": cfg.openai.voice,
        "response_format": "mp3",
    });
    post_bytes(
        "https://api.openai.com/v1/audio/speech",
        &[
            ("Authorization", &format!("Bearer {key}")),
            ("Content-Type", "application/json"),
        ],
        &body,
    )
}

fn synth_elevenlabs(cfg: &VoiceConfig, text: &str) -> Result<Vec<u8>> {
    let key = env_key(&cfg.elevenlabs.api_key_env).ok_or(VoiceError::NoProvider)?;
    let voice = cfg.elevenlabs.voice_id.trim();
    if voice.is_empty() {
        return Err(VoiceError::Config(
            "elevenlabs.voice_id is required in voice.toml".into(),
        ));
    }
    let url = format!("https://api.elevenlabs.io/v1/text-to-speech/{voice}");
    let body = serde_json::json!({
        "text": text,
        "model_id": cfg.elevenlabs.model_id,
    });
    post_bytes(
        &url,
        &[
            ("xi-api-key", key.as_str()),
            ("Content-Type", "application/json"),
            ("Accept", "audio/mpeg"),
        ],
        &body,
    )
}

fn post_bytes(
    url: &str,
    headers: &[(&str, &str)],
    body: &serde_json::Value,
) -> Result<Vec<u8>> {
    let mut req = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(60))
        .build()
        .map_err(|e| VoiceError::Http(e.to_string()))?
        .post(url)
        .json(body);
    for (k, v) in headers {
        req = req.header(*k, *v);
    }
    let resp = req.send().map_err(|e| VoiceError::Http(e.to_string()))?;
    let status = resp.status();
    let bytes = resp
        .bytes()
        .map_err(|e| VoiceError::Http(e.to_string()))?;
    if !status.is_success() {
        let msg = String::from_utf8_lossy(&bytes);
        return Err(VoiceError::Http(format!("{status}: {msg}")));
    }
    Ok(bytes.to_vec())
}

/// Best-effort local playback of mp3.
pub fn play_mp3(path: &Path) -> Result<()> {
    let candidates = [
        (
            "ffplay",
            vec![
                "-nodisp".into(),
                "-autoexit".into(),
                "-loglevel".into(),
                "quiet".into(),
                path.display().to_string(),
            ],
        ),
        (
            "mpv",
            vec![
                "--no-video".into(),
                "--really-quiet".into(),
                path.display().to_string(),
            ],
        ),
        ("mpg123", vec!["-q".into(), path.display().to_string()]),
    ];
    for (bin, args) in candidates {
        if Command::new(bin).args(&args).status().map(|s| s.success()).unwrap_or(false) {
            return Ok(());
        }
    }
    Err(VoiceError::Playback(format!(
        "no player worked for {} (install ffplay, mpv, or mpg123)",
        path.display()
    )))
}

/// Build short spoken lines from a tick's new events (high signal only).
pub fn notify_lines_from_events(events: &[Event]) -> Vec<(String, SpeakIntent)> {
    let mut out = Vec::new();
    for e in events {
        match &e.body {
            EventBody::StoryOpened { title, .. } => {
                out.push((
                    format!("Attention. Story opened. {title}"),
                    SpeakIntent::Pet,
                ));
            }
            EventBody::StoryClosed { summary, .. } => {
                out.push((
                    format!("Story closed. {summary}"),
                    SpeakIntent::Pet,
                ));
            }
            EventBody::XpGranted {
                skill_id,
                amount,
                reason,
                ..
            } => {
                out.push((
                    format!("Park skill {skill_id} plus {amount}. {reason}"),
                    SpeakIntent::Default,
                ));
            }
            EventBody::MemoryFormed {
                class, title, ..
            } => {
                // Only speak high-importance-ish episodic titles (all system ones for v1)
                if matches!(
                    class,
                    waga_core::MemoryClass::Episodic | waga_core::MemoryClass::Affective
                ) {
                    out.push((
                        format!("Memory. {title}"),
                        SpeakIntent::Fast,
                    ));
                }
            }
            _ => {}
        }
    }
    // Coalesce: max 3 lines per tick
    out.truncate(3);
    out
}

/// Speak all notify lines (best effort; logs errors).
pub fn speak_notify_lines(
    cfg: &VoiceConfig,
    data_dir: &Path,
    lines: &[(String, SpeakIntent)],
) {
    if !cfg.enabled || lines.is_empty() {
        return;
    }
    for (text, intent) in lines {
        match speak(cfg, text, *intent, data_dir) {
            Ok(p) => tracing::info!("voice ok → {}", p.display()),
            Err(e) => tracing::warn!("voice skip: {e}"),
        }
    }
}

/// Example voice.toml content for docs.
pub fn example_voice_toml() -> &'static str {
    r#"# WAGA premium TTS — tri-provider
enabled = true
default_provider = "auto"  # auto | xai | openai | elevenlabs | null
play = true

[auto]
notify_pet = "elevenlabs"
notify_fast = "openai"
notify_default = "xai"
fallback = ["xai", "openai", "elevenlabs"]

[xai]
api_key_env = "XAI_API_KEY"
voice_id = "eve"
language = "en"

[openai]
api_key_env = "OPENAI_API_KEY"
model = "tts-1-hd"
voice = "nova"

[elevenlabs]
api_key_env = "ELEVENLABS_API_KEY"
voice_id = ""   # paste your ElevenLabs voice id
model_id = "eleven_multilingual_v2"
"#
}

#[cfg(test)]
mod tests {
    use super::*;
    use waga_core::{Event, EventBody, EventId, StoryId};

    fn ev(body: EventBody) -> Event {
        Event {
            id: EventId("evt_t".into()),
            tick: 1,
            at: waga_core::WorldSnapshot::fresh("t").observed_at,
            actor: "test".into(),
            links: vec![],
            body,
        }
    }

    #[test]
    fn notify_lines_from_story_events() {
        let events = vec![
            ev(EventBody::StoryOpened {
                story_id: StoryId("s".into()),
                title: "Working tree dirty on main".into(),
            }),
            ev(EventBody::TickStarted),
            ev(EventBody::XpGranted {
                skill_id: "repo_hygiene".into(),
                amount: 10,
                beneficiary: waga_core::XpBeneficiary::Park,
                memory_id: None,
                reason: "clean".into(),
            }),
        ];
        let lines = notify_lines_from_events(&events);
        assert!(lines.len() >= 2);
        assert!(lines[0].0.contains("Story opened"));
    }

    #[test]
    fn resolve_null() {
        let mut cfg = VoiceConfig::default();
        cfg.default_provider = VoiceProvider::Null;
        assert_eq!(
            resolve_provider(&cfg, SpeakIntent::Default).unwrap(),
            VoiceProvider::Null
        );
    }

    #[test]
    fn example_toml_parses() {
        let cfg: VoiceConfig = toml::from_str(example_voice_toml()).unwrap();
        assert!(cfg.enabled);
        assert_eq!(cfg.auto.notify_default, VoiceProvider::Xai);
    }

    #[test]
    fn speak_null_writes_empty_ok() {
        let dir = tempfile::tempdir().unwrap();
        let mut cfg = VoiceConfig::default();
        cfg.default_provider = VoiceProvider::Null;
        cfg.play = false;
        let p = speak(&cfg, "hello park", SpeakIntent::Default, dir.path()).unwrap();
        assert!(p.exists());
    }
}
