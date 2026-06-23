use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PersonalEntryKind {
    Glossary,
    UserLexicon,
    Contact,
    EmojiHistory,
    CorrectionHistory,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PersonalEntry {
    pub kind: PersonalEntryKind,
    pub reading: String,
    pub text: String,
    pub trust: f32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct PersonalKnowledgeStore {
    entries: Vec<PersonalEntry>,
}

impl PersonalKnowledgeStore {
    pub fn new(entries: Vec<PersonalEntry>) -> Self {
        Self { entries }
    }

    pub fn compile(&self) -> PersonalPredictiveCache {
        let mut cache = PersonalPredictiveCache::default();
        for entry in &self.entries {
            cache
                .bonuses
                .insert((entry.reading.clone(), entry.text.clone()), entry.trust);
        }
        cache
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct PersonalPredictiveCache {
    /// Static, compiled-in overlays (glossary, contacts, …).
    bonuses: HashMap<(String, String), f32>,
    /// Adaptive user dictionary: times the user committed (reading, text).
    /// Learned at runtime, persisted by the host, folded into `score_bonus`.
    user_counts: HashMap<(String, String), u32>,
}

/// Score bonus for a learned (reading, text) given how often it was committed.
/// Diminishing returns (log) and capped so a learned word nudges ranking but
/// never overrides geometry/observation evidence.
fn user_commit_bonus(count: u32) -> f32 {
    if count == 0 {
        0.0
    } else {
        ((count as f32).ln_1p() * 1.2).min(4.0)
    }
}

impl PersonalPredictiveCache {
    pub fn score_bonus(&self, reading: &str, text: &str) -> f32 {
        let key = (reading.to_owned(), text.to_owned());
        let static_bonus = self.bonuses.get(&key).copied().unwrap_or(0.0);
        let learned = self
            .user_counts
            .get(&key)
            .map(|&count| user_commit_bonus(count))
            .unwrap_or(0.0);
        static_bonus + learned
    }

    /// Record a user commit, raising the adaptive bonus for this (reading, text).
    pub fn learn_commit(&mut self, reading: &str, text: &str) {
        if text.is_empty() {
            return;
        }
        *self
            .user_counts
            .entry((reading.to_owned(), text.to_owned()))
            .or_insert(0) += 1;
    }

    /// Export the learned user dictionary for persistence: (reading, text, count).
    pub fn user_dictionary(&self) -> Vec<(String, String, u32)> {
        self.user_counts
            .iter()
            .map(|((reading, text), &count)| (reading.clone(), text.clone(), count))
            .collect()
    }

    /// Restore a persisted user dictionary (replaces current learned counts).
    pub fn load_user_dictionary<I>(&mut self, entries: I)
    where
        I: IntoIterator<Item = (String, String, u32)>,
    {
        self.user_counts = entries
            .into_iter()
            .filter(|(_, text, count)| !text.is_empty() && *count > 0)
            .map(|(reading, text, count)| ((reading, text), count))
            .collect();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn learning_raises_bonus_and_persists() {
        let mut cache = PersonalPredictiveCache::default();
        assert_eq!(cache.score_bonus("ni hao", "你好"), 0.0);

        cache.learn_commit("ni hao", "你好");
        let after_one = cache.score_bonus("ni hao", "你好");
        assert!(after_one > 0.0);

        cache.learn_commit("ni hao", "你好");
        assert!(cache.score_bonus("ni hao", "你好") > after_one);

        // empty commits are ignored
        cache.learn_commit("x", "");
        assert!(cache.user_dictionary().iter().all(|(_, t, _)| !t.is_empty()));

        // persistence round-trip preserves the learned bonus
        let dumped = cache.user_dictionary();
        let mut restored = PersonalPredictiveCache::default();
        restored.load_user_dictionary(dumped);
        assert_eq!(
            restored.score_bonus("ni hao", "你好"),
            cache.score_bonus("ni hao", "你好")
        );
    }
}
