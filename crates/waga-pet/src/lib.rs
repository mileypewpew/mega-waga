//! Waga pet — terminal companion mood and sprites.

use std::fmt;
use waga_core::WorldSnapshot;

/// Derived mood — never invents world facts, only maps them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PetMood {
    /// Git tree is dirty.
    Grumpy,
    /// Git tree is clean.
    Content,
    /// No git facts available.
    Idle,
}

impl PetMood {
    pub fn as_str(self) -> &'static str {
        match self {
            PetMood::Grumpy => "grumpy",
            PetMood::Content => "content",
            PetMood::Idle => "idle",
        }
    }
}

impl fmt::Display for PetMood {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Map park truth → pet mood.
pub fn mood_from_snapshot(snapshot: &WorldSnapshot) -> PetMood {
    match &snapshot.git {
        Some(g) if g.dirty => PetMood::Grumpy,
        Some(_) => PetMood::Content,
        None => PetMood::Idle,
    }
}

/// Multi-line Unicode sprite for the given mood.
pub fn sprite(mood: PetMood) -> &'static str {
    match mood {
        PetMood::Grumpy => {
            r#"
    /\_/\
   ( >_< )   ~ grumpy ~
   /  ~  \   dirty tree!
  (__|_|__)
"#
        }
        PetMood::Content => {
            r#"
    /\_/\
   ( ^_^ )   ~ content ~
   /  U  \   all clean
  (__|_|__)
"#
        }
        PetMood::Idle => {
            r#"
    /\_/\
   ( o_o )   ~ idle ~
   /  -  \   looking around
  (__|_|__)
"#
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use waga_core::{GitStatus, WorldSnapshot};

    #[test]
    fn dirty_is_grumpy() {
        let mut s = WorldSnapshot::fresh("strict-cto");
        s.git = Some(GitStatus {
            repo_path: ".".into(),
            branch: "main".into(),
            dirty: true,
        });
        assert_eq!(mood_from_snapshot(&s), PetMood::Grumpy);
        assert!(sprite(PetMood::Grumpy).contains("grumpy"));
    }

    #[test]
    fn clean_is_content() {
        let mut s = WorldSnapshot::fresh("strict-cto");
        s.git = Some(GitStatus {
            repo_path: ".".into(),
            branch: "main".into(),
            dirty: false,
        });
        assert_eq!(mood_from_snapshot(&s), PetMood::Content);
    }

    #[test]
    fn no_git_is_idle() {
        let s = WorldSnapshot::fresh("strict-cto");
        assert_eq!(mood_from_snapshot(&s), PetMood::Idle);
    }
}
