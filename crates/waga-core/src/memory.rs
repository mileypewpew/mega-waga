//! Classified memories and park-level skill XP types.

use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};

use crate::EventId;
use crate::StoryId;

/// Stable memory identifier (`mem_<uuid>`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MemoryId(pub String);

impl MemoryId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for MemoryId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Primary memory classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryClass {
    Episodic,
    Semantic,
    Procedural,
    Affective,
    Preference,
    Working,
}

impl MemoryClass {
    pub fn as_str(self) -> &'static str {
        match self {
            MemoryClass::Episodic => "episodic",
            MemoryClass::Semantic => "semantic",
            MemoryClass::Procedural => "procedural",
            MemoryClass::Affective => "affective",
            MemoryClass::Preference => "preference",
            MemoryClass::Working => "working",
        }
    }
}

impl std::fmt::Display for MemoryClass {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Who/what formed the memory.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemorySource {
    SystemRule,
    User,
    Persona,
    Llm,
}

/// Scope of the memory (v1: park only for XP pairing).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryScope {
    Park,
}

/// Durable classified memory (subjective; cites events).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Memory {
    pub id: MemoryId,
    pub class: MemoryClass,
    pub scope: MemoryScope,
    pub title: String,
    pub body: String,
    pub tags: Vec<String>,
    pub importance: u8,
    pub confidence: f32,
    pub source: MemorySource,
    pub source_event_ids: Vec<EventId>,
    pub story_id: Option<StoryId>,
    pub formed_at_tick: u64,
    pub formed_at: DateTime<Local>,
}

/// Who receives XP (v1: park/user sheet only).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum XpBeneficiary {
    #[default]
    Park,
}

/// Projected skill totals for the park.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillState {
    pub skill_id: String,
    pub xp: u64,
    pub level: u32,
}

/// Derive level from total XP (simple table).
pub fn level_from_xp(xp: u64) -> u32 {
    const THRESHOLDS: &[u64] = &[0, 20, 50, 100, 200, 400, 800, 1600];
    let mut level = 0u32;
    for (i, t) in THRESHOLDS.iter().enumerate() {
        if xp >= *t {
            level = i as u32;
        }
    }
    level
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn level_thresholds() {
        assert_eq!(level_from_xp(0), 0);
        assert_eq!(level_from_xp(19), 0);
        assert_eq!(level_from_xp(20), 1);
        assert_eq!(level_from_xp(50), 2);
        assert_eq!(level_from_xp(100), 3);
    }
}
