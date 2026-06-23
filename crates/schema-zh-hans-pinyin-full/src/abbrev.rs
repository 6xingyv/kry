//! 简拼 (abbreviation) support: map syllable-initials (东西 "dong xi" → "dx") to
//! lexicon entries, and turn an all-initials input into candidates ranked below
//! full pinyin via `ABBREV_PENALTY`.

use std::collections::HashMap;

use schema_core::{Boundary, CandidateSource, ReadingCandidate};

use super::{
    ABBREV_MAX_SYLLABLES, ABBREV_MIN_SYLLABLES, ABBREV_PENALTY, ABBREV_PER_KEY,
    ZhHansPinyinFullSchema,
};

impl ZhHansPinyinFullSchema {
    /// Lazily built initials -> entry-index map for 简拼 (abbreviation) input.
    /// Built once on first abbreviation lookup by scanning the lexicon; for each
    /// multi-syllable word we key on the first letter of every syllable
    /// (东西 "dong xi" -> "dx"), keep the cheapest (most frequent) entries per key.
    pub(super) fn abbrev_index(&self) -> &HashMap<String, Vec<u32>> {
        self.abbrev_index.get_or_init(|| {
            let mut map: HashMap<String, Vec<(u32, f32)>> = HashMap::new();
            for idx in 0..self.lexicon.entry_count() {
                let Some(entry) = self.lexicon.entry(idx) else {
                    continue;
                };
                let syllables = entry.reading.split_whitespace().collect::<Vec<_>>();
                if syllables.len() < ABBREV_MIN_SYLLABLES || syllables.len() > ABBREV_MAX_SYLLABLES {
                    continue;
                }
                let initials: String = syllables
                    .iter()
                    .filter_map(|s| s.chars().next())
                    .collect();
                if initials.len() != syllables.len() {
                    continue; // skip readings with empty/odd syllables
                }
                let cost = self.lexicon.entry_cost(&entry);
                map.entry(initials).or_default().push((idx as u32, cost));
            }
            map.into_iter()
                .map(|(key, mut entries)| {
                    entries.sort_by(|a, b| a.1.total_cmp(&b.1));
                    entries.truncate(ABBREV_PER_KEY);
                    (key, entries.into_iter().map(|(idx, _)| idx).collect())
                })
                .collect()
        })
    }

    /// 简拼: treat the whole input as syllable-initials (dx -> 东西). Ranked below
    /// full pinyin via ABBREV_PENALTY but above raw fallback.
    pub(super) fn abbreviation_candidates(&self, normalized: &str) -> Vec<ReadingCandidate> {
        if normalized.len() < ABBREV_MIN_SYLLABLES
            || normalized.len() > ABBREV_MAX_SYLLABLES
            || !normalized.chars().all(|c| c.is_ascii_lowercase())
        {
            return Vec::new();
        }
        let Some(indices) = self.abbrev_index().get(normalized) else {
            return Vec::new();
        };
        let mut candidates = Vec::new();
        for &idx in indices {
            let Some(entry) = self.lexicon.entry(idx as usize) else {
                continue;
            };
            let cost = self.lexicon.entry_cost(&entry) + ABBREV_PENALTY;
            candidates.push(ReadingCandidate {
                reading: entry.reading.clone(),
                boundary: Boundary::from_reading(&entry.reading),
                text: entry.text,
                cost,
                source: CandidateSource::Prefix,
            });
        }
        candidates.sort_by(|a, b| a.cost.total_cmp(&b.cost).then_with(|| a.text.cmp(&b.text)));
        candidates.truncate(16);
        candidates
    }
}
