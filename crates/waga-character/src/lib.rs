//! Persona files and template-based notices (no LLM in v0).

use serde::Deserialize;
use std::fs;
use std::path::Path;
use waga_core::{Result, WagaError, WorldSnapshot};

/// On-disk persona definition (TOML).
#[derive(Debug, Clone, Deserialize)]
pub struct Persona {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub voice: String,
    #[serde(default)]
    pub constraints: Vec<String>,
    pub templates: PersonaTemplates,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PersonaTemplates {
    pub git_dirty: String,
    pub git_clean: String,
    pub default: String,
}

impl Persona {
    /// Load a persona TOML file from disk.
    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        let text = fs::read_to_string(path.as_ref())?;
        toml::from_str(&text).map_err(|e| WagaError::Toml(e.to_string()))
    }

    /// Produce an in-character notice from the current world snapshot.
    pub fn notice(&self, snapshot: &WorldSnapshot) -> String {
        self.notice_with_memories(snapshot, &[])
    }

    /// Notice plus optional recent memory titles (park learning context).
    pub fn notice_with_memories(
        &self,
        snapshot: &WorldSnapshot,
        recent_memory_titles: &[&str],
    ) -> String {
        let template = match &snapshot.git {
            Some(g) if g.dirty => &self.templates.git_dirty,
            Some(_) => &self.templates.git_clean,
            None => &self.templates.default,
        };
        let mut base = fill_template(template, snapshot);
        if let Some(extra) = format_memory_aside(recent_memory_titles) {
            base.push(' ');
            base.push_str(&extra);
        }
        base
    }
}

/// Strict CTO-style aside from recent park memories (no invented facts).
fn format_memory_aside(titles: &[&str]) -> Option<String> {
    if titles.is_empty() {
        return None;
    }
    let n = titles.len().min(2);
    let joined = titles[..n].join("; ");
    Some(format!("(recalling: {joined})"))
}

fn fill_template(template: &str, snapshot: &WorldSnapshot) -> String {
    let branch = snapshot
        .git
        .as_ref()
        .map(|g| g.branch.as_str())
        .unwrap_or("unknown");
    template
        .replace("{branch}", branch)
        .replace("{tick}", &snapshot.tick.to_string())
        .replace("{persona}", &snapshot.active_persona)
        .replace("{name}", &snapshot.active_persona)
}

/// Built-in Strict CTO used when no persona file is found.
pub fn strict_cto_builtin() -> Persona {
    Persona {
        id: "strict-cto".into(),
        name: "Strict CTO".into(),
        voice: "terse, high standards, no fluff".into(),
        constraints: vec!["Never invent git or filesystem facts".into()],
        templates: PersonaTemplates {
            git_dirty: "Repo dirty on {branch}. Clean tree before we talk merge.".into(),
            git_clean: "Tree clean on {branch}. Good.".into(),
            default: "Tick {tick}. Standing by.".into(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use waga_core::{GitStatus, WorldSnapshot};

    fn snap_with_git(dirty: bool) -> WorldSnapshot {
        let mut s = WorldSnapshot::fresh("strict-cto");
        s.tick = 3;
        s.git = Some(GitStatus {
            repo_path: "/tmp/demo".into(),
            branch: "main".into(),
            dirty,
        });
        s
    }

    #[test]
    fn dirty_repo_uses_git_dirty_template() {
        let p = strict_cto_builtin();
        let notice = p.notice(&snap_with_git(true));
        assert!(notice.contains("dirty"));
        assert!(notice.contains("main"));
    }

    #[test]
    fn clean_repo_uses_git_clean_template() {
        let p = strict_cto_builtin();
        let notice = p.notice(&snap_with_git(false));
        assert!(notice.contains("clean"));
        assert!(notice.contains("main"));
    }

    #[test]
    fn no_git_uses_default_template() {
        let p = strict_cto_builtin();
        let mut s = WorldSnapshot::fresh("strict-cto");
        s.tick = 7;
        let notice = p.notice(&s);
        assert!(notice.contains("7"));
        assert!(notice.contains("Standing by"));
    }

    #[test]
    fn notice_includes_memory_aside() {
        let p = strict_cto_builtin();
        let notice = p.notice_with_memories(
            &snap_with_git(false),
            &["Tree clean on main", "Pet recovered"],
        );
        assert!(notice.contains("clean"));
        assert!(notice.contains("recalling:"));
        assert!(notice.contains("Tree clean on main"));
    }

    #[test]
    fn load_toml_from_disk() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("strict-cto.toml");
        fs::write(
            &path,
            r#"
id = "strict-cto"
name = "Strict CTO"
voice = "terse"
constraints = ["Never invent facts"]

[templates]
git_dirty = "DIRTY {branch}"
git_clean = "CLEAN {branch}"
default = "DEFAULT {tick}"
"#,
        )
        .unwrap();
        let p = Persona::load(&path).unwrap();
        assert_eq!(p.id, "strict-cto");
        assert_eq!(p.templates.git_dirty, "DIRTY {branch}");
    }
}
