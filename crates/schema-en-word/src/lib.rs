mod prefix;

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

use data_core::{Lexicon, LexiconEntry};
use schema_core::{
    Boundary, CandidateSource, IncrementalSchema, ReadingCandidate, Schema, SchemaAdvanceResult,
    SchemaId, SchemaStateId,
};

use prefix::EnglishPrefixIndex;

#[derive(Clone, Debug)]
struct EnglishInternalState {
    trie_position: usize,
    accumulated: String,
}

#[derive(Clone, Debug)]
pub struct EnglishWordSchema {
    lexicon: Lexicon,
    prefix_index: EnglishPrefixIndex,
    // Built lazily on first fuzzy lookup: the SymSpell delete-key index over ~98k
    // English words costs ~1.2s to build, and in Chinese mode (where en-word is a
    // suppressed fallback) it is rarely if ever used. Deferring it removes that
    // cost from startup — the "don't build until used" lesson from librime's
    // memory-mapped, load-on-demand dictionaries.
    fuzzy_index: RefCell<Option<EnglishFuzzyIndex>>,
    arena: RefCell<Vec<EnglishInternalState>>,
}

impl EnglishWordSchema {
    pub fn new(lexicon: Lexicon) -> Self {
        let prefix_index = EnglishPrefixIndex::build(lexicon.iter_entries(), 64);
        Self {
            lexicon,
            prefix_index,
            fuzzy_index: RefCell::new(None),
            arena: RefCell::new(Vec::new()),
        }
    }

    /// Fuzzy candidates, building the delete-key index on first use.
    fn fuzzy_candidates(&self, normalized: &str) -> Vec<ReadingCandidate> {
        let mut guard = self.fuzzy_index.borrow_mut();
        let index = guard.get_or_insert_with(|| EnglishFuzzyIndex::build(self.lexicon.iter_entries()));
        fuzzy_candidates_impl(&self.lexicon, index, normalized)
    }

    pub fn builtin() -> Self {
        Self::new(Lexicon::new([
            LexiconEntry::new("the", "the", 1000.0),
            LexiconEntry::new("hello", "hello", 900.0),
            LexiconEntry::new("layout", "layout", 700.0),
            LexiconEntry::new("keyboard", "keyboard", 650.0),
            LexiconEntry::new("decoder", "decoder", 600.0),
        ]))
    }
}

impl Default for EnglishWordSchema {
    fn default() -> Self {
        Self::builtin()
    }
}

impl Schema for EnglishWordSchema {
    fn id(&self) -> SchemaId {
        SchemaId::new("en-word")
    }

    fn candidates(&self, symbol_path: &str) -> Vec<ReadingCandidate> {
        let normalized = normalize_english(symbol_path);
        if normalized.is_empty() {
            return Vec::new();
        }

        let exact = exact_candidates(&self.lexicon, &normalized);
        let prefix = prefix_candidates(&self.lexicon, &self.prefix_index, &normalized);
        let mut candidates = exact;
        if prefix.is_empty() || !candidates.is_empty() {
            candidates.extend(self.fuzzy_candidates(&normalized));
        }
        candidates.extend(prefix);

        candidates.push(ReadingCandidate {
            reading: normalized.clone(),
            boundary: Boundary::from_reading(&normalized),
            text: normalized,
            cost: 6.0,
            source: CandidateSource::Raw,
        });
        candidates.sort_by(|a, b| {
            a.cost
                .total_cmp(&b.cost)
                .then_with(|| a.text.cmp(&b.text))
                .then_with(|| a.reading.cmp(&b.reading))
        });
        candidates
    }

    fn as_incremental(&self) -> Option<&dyn IncrementalSchema> {
        Some(self)
    }
}

impl EnglishWordSchema {
    fn alloc_state(&self, internal: EnglishInternalState) -> u32 {
        let mut arena = self.arena.borrow_mut();
        let index = arena.len() as u32;
        arena.push(internal);
        index
    }

    fn get_state(&self, index: u32) -> EnglishInternalState {
        self.arena.borrow()[index as usize].clone()
    }
}

impl IncrementalSchema for EnglishWordSchema {
    fn initial_state(&self) -> SchemaStateId {
        let index = self.alloc_state(EnglishInternalState {
            trie_position: self.prefix_index.root(),
            accumulated: String::new(),
        });
        SchemaStateId {
            state_index: index,
            accumulated_cost: 0.0,
            symbol_count: 0,
            alive: true,
        }
    }

    fn advance(&self, state: &SchemaStateId, symbol: char) -> Vec<SchemaAdvanceResult> {
        if !state.alive {
            return Vec::new();
        }

        let internal = self.get_state(state.state_index);
        let ch = symbol.to_ascii_lowercase();
        if !ch.is_ascii_alphabetic() {
            return Vec::new();
        }

        let mut accumulated = internal.accumulated.clone();
        accumulated.push(ch);

        let (next_trie_pos, alive) =
            if let Some(next) = self.prefix_index.advance_char(internal.trie_position, ch) {
                (next, self.prefix_index.has_children(next))
            } else {
                (internal.trie_position, false)
            };

        let completed = exact_candidates(&self.lexicon, &accumulated);

        let next_index = self.alloc_state(EnglishInternalState {
            trie_position: next_trie_pos,
            accumulated,
        });

        let alive = alive || !completed.is_empty();

        vec![SchemaAdvanceResult {
            next_state: SchemaStateId {
                state_index: next_index,
                accumulated_cost: state.accumulated_cost,
                symbol_count: state.symbol_count + 1,
                alive,
            },
            cost_delta: 0.0,
            completed,
        }]
    }

    fn candidates_at(&self, state: &SchemaStateId) -> Vec<ReadingCandidate> {
        let internal = self.get_state(state.state_index);
        if internal.accumulated.is_empty() {
            return Vec::new();
        }

        let mut candidates =
            prefix_candidates(&self.lexicon, &self.prefix_index, &internal.accumulated);
        candidates.extend(exact_candidates(&self.lexicon, &internal.accumulated));
        candidates.extend(self.fuzzy_candidates(&internal.accumulated));

        candidates.push(ReadingCandidate {
            reading: internal.accumulated.clone(),
            boundary: Boundary::from_reading(&internal.accumulated),
            text: internal.accumulated,
            cost: 6.0,
            source: CandidateSource::Raw,
        });
        candidates.sort_by(|a, b| {
            a.cost
                .total_cmp(&b.cost)
                .then_with(|| a.text.cmp(&b.text))
                .then_with(|| a.reading.cmp(&b.reading))
        });
        candidates
    }

    fn reset_arena(&self) {
        self.arena.borrow_mut().clear();
    }
}

fn exact_candidates(lexicon: &Lexicon, normalized: &str) -> Vec<ReadingCandidate> {
    lexicon
        .lookup_reading(normalized)
        .iter()
        .map(|entry| ReadingCandidate {
            reading: normalized.to_owned(),
            boundary: Boundary::from_reading(normalized),
            text: entry.text.clone(),
            cost: lexical_prior_cost(lexicon.entry_cost(entry)),
            source: CandidateSource::Exact,
        })
        .collect()
}

#[derive(Clone, Debug, Default)]
struct EnglishFuzzyIndex {
    delete_keys: HashMap<String, Vec<usize>>,
}

impl EnglishFuzzyIndex {
    fn build(entries: impl Iterator<Item = LexiconEntry>) -> Self {
        let mut delete_keys: HashMap<String, Vec<usize>> = HashMap::new();
        for (idx, entry) in entries.enumerate() {
            for key in fuzzy_delete_keys(&entry.reading, max_fuzzy_distance(entry.reading.len())) {
                delete_keys.entry(key).or_default().push(idx);
            }
        }
        Self { delete_keys }
    }

    fn lookup(&self, normalized: &str, max_distance: usize) -> Vec<usize> {
        let mut seen = HashSet::new();
        let mut indices = Vec::new();
        for key in fuzzy_delete_keys(normalized, max_distance) {
            let Some(bucket) = self.delete_keys.get(&key) else {
                continue;
            };
            for index in bucket {
                if seen.insert(*index) {
                    indices.push(*index);
                }
            }
        }
        indices
    }
}

fn fuzzy_candidates_impl(
    lexicon: &Lexicon,
    fuzzy_index: &EnglishFuzzyIndex,
    normalized: &str,
) -> Vec<ReadingCandidate> {
    if normalized.len() < 3 {
        return Vec::new();
    }

    let max_distance = max_fuzzy_distance(normalized.len());
    let max_edit_cost = if normalized.len() >= 7 {
        EDIT_DELETE_COST * 2
    } else {
        EDIT_DELETE_COST
    };
    let mut candidates = Vec::new();
    let mut entry_indices = fuzzy_index.lookup(normalized, max_distance);
    entry_indices.sort_by(|left, right| {
        let left_cost = lexicon
            .entry(*left)
            .map(|entry| lexicon.entry_cost(&entry))
            .unwrap_or(f32::INFINITY);
        let right_cost = lexicon
            .entry(*right)
            .map(|entry| lexicon.entry_cost(&entry))
            .unwrap_or(f32::INFINITY);
        left_cost
            .total_cmp(&right_cost)
            .then_with(|| left.cmp(right))
    });
    for entry_index in entry_indices {
        let Some(entry) = lexicon.entry(entry_index) else {
            continue;
        };
        let Some(edit_cost) = edit_cost_limited(normalized, &entry.reading, max_edit_cost) else {
            continue;
        };
        if edit_cost == 0 {
            continue;
        }
        candidates.push(ReadingCandidate {
            reading: entry.reading.clone(),
            boundary: Boundary::from_reading(&entry.reading),
            text: entry.text.clone(),
            cost: edit_cost as f32 / 100.0 + frequency_tiebreak(lexicon.entry_cost(&entry)),
            source: CandidateSource::Fuzzy,
        });
        if candidates.len() >= 64 {
            break;
        }
    }
    candidates.sort_by(|a, b| {
        a.cost
            .total_cmp(&b.cost)
            .then_with(|| a.text.cmp(&b.text))
            .then_with(|| a.reading.cmp(&b.reading))
    });
    candidates.truncate(16);
    candidates
}

fn max_fuzzy_distance(len: usize) -> usize {
    if len >= 7 { 2 } else { 1 }
}

fn fuzzy_delete_keys(value: &str, max_distance: usize) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    fuzzy_delete_keys_from(value.as_bytes(), max_distance, &mut seen, &mut out);
    out
}

fn fuzzy_delete_keys_from(
    value: &[u8],
    remaining: usize,
    seen: &mut HashSet<String>,
    out: &mut Vec<String>,
) {
    if let Ok(key) = std::str::from_utf8(value) {
        if seen.insert(key.to_owned()) {
            out.push(key.to_owned());
        }
    }
    if remaining == 0 || value.is_empty() {
        return;
    }
    for index in 0..value.len() {
        let mut deleted = Vec::with_capacity(value.len() - 1);
        deleted.extend_from_slice(&value[..index]);
        deleted.extend_from_slice(&value[index + 1..]);
        fuzzy_delete_keys_from(&deleted, remaining - 1, seen, out);
    }
}

fn prefix_candidates(
    lexicon: &Lexicon,
    prefix_index: &EnglishPrefixIndex,
    normalized: &str,
) -> Vec<ReadingCandidate> {
    if normalized.len() < 2 {
        return Vec::new();
    }

    let mut candidates = Vec::new();
    for entry_index in prefix_index.lookup(normalized, 24) {
        let Some(entry) = lexicon.entry(*entry_index) else {
            continue;
        };
        if entry.reading == normalized {
            continue;
        }
        let missing = entry.reading.len().saturating_sub(normalized.len());
        candidates.push(ReadingCandidate {
            reading: entry.reading.clone(),
            boundary: Boundary::from_reading(&entry.reading),
            text: entry.text.clone(),
            cost: 2.0 + missing as f32 * 0.08 + frequency_tiebreak(lexicon.entry_cost(&entry)),
            source: CandidateSource::Prefix,
        });
    }
    candidates.sort_by(|a, b| {
        a.cost
            .total_cmp(&b.cost)
            .then_with(|| a.text.cmp(&b.text))
            .then_with(|| a.reading.cmp(&b.reading))
    });
    candidates.truncate(8);
    candidates
}

fn normalize_english(value: &str) -> String {
    value
        .chars()
        .filter(|ch| ch.is_ascii_alphabetic())
        .map(|ch| ch.to_ascii_lowercase())
        .collect()
}

fn frequency_tiebreak(cost: f32) -> f32 {
    cost.max(0.0) * 0.01
}

fn lexical_prior_cost(cost: f32) -> f32 {
    cost.max(0.0) * 0.05
}

const EDIT_TRANSPOSE_COST: u16 = 45;
const EDIT_SUBSTITUTE_COST: u16 = 55;
const EDIT_DELETE_COST: u16 = 80;
const EDIT_INSERT_COST: u16 = 80;

fn edit_cost_limited(lhs: &str, rhs: &str, limit: u16) -> Option<u16> {
    if lhs == rhs {
        return Some(0);
    }
    if lhs.len().abs_diff(rhs.len()) as u16 * EDIT_DELETE_COST > limit {
        return None;
    }

    let left = lhs.as_bytes();
    let right = rhs.as_bytes();
    let max = u16::MAX / 4;
    let mut prev_prev = vec![max; right.len() + 1];
    let mut prev = (0..=right.len())
        .map(|index| (index as u16).saturating_mul(EDIT_INSERT_COST))
        .collect::<Vec<_>>();
    let mut current = vec![0; right.len() + 1];

    for i in 1..=left.len() {
        current[0] = (i as u16).saturating_mul(EDIT_DELETE_COST);
        let mut row_min = current[0];
        for j in 1..=right.len() {
            let substitution_cost = if left[i - 1] == right[j - 1] {
                0
            } else {
                EDIT_SUBSTITUTE_COST
            };
            let mut best = (current[j - 1] + EDIT_INSERT_COST)
                .min(prev[j] + EDIT_DELETE_COST)
                .min(prev[j - 1] + substitution_cost);
            if i > 1 && j > 1 && left[i - 1] == right[j - 2] && left[i - 2] == right[j - 1] {
                best = best.min(prev_prev[j - 2] + EDIT_TRANSPOSE_COST);
            }
            current[j] = best;
            row_min = row_min.min(best);
        }
        if row_min > limit {
            return None;
        }
        std::mem::swap(&mut prev_prev, &mut prev);
        std::mem::swap(&mut prev, &mut current);
    }

    (prev[right.len()] <= limit).then_some(prev[right.len()])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_candidates_use_frequency_cost() {
        let schema = EnglishWordSchema::new(Lexicon::new([
            LexiconEntry::new("low", "same", 1.0),
            LexiconEntry::new("high", "same", 100.0),
        ]));
        let candidates = schema.candidates("same");
        assert_eq!(candidates[0].text, "high");
        assert!(candidates[0].cost < candidates[1].cost);
    }

    #[test]
    fn fuzzy_candidates_correct_adjacent_transposition() {
        let schema = EnglishWordSchema::builtin();
        let candidates = schema.candidates("teh");
        assert_eq!(candidates[0].text, "the");
        assert!(candidates[0].cost < 6.0);
    }

    #[test]
    fn fuzzy_candidates_correct_single_substitution() {
        let schema = EnglishWordSchema::builtin();
        let candidates = schema.candidates("keyboarf");
        assert_eq!(candidates[0].text, "keyboard");
        assert!(candidates[0].cost < 6.0);
    }

    #[test]
    fn prefix_candidates_complete_partial_words() {
        let schema = EnglishWordSchema::builtin();
        let candidates = schema.candidates("keyb");
        assert_eq!(candidates[0].text, "keyboard");
        assert_eq!(candidates[0].reading, "keyboard");
        assert!(candidates[0].cost < 6.0);
    }

    #[test]
    fn exact_short_word_stays_ahead_of_completion() {
        let schema = EnglishWordSchema::new(Lexicon::new([
            LexiconEntry::new("in", "in", 100.0),
            LexiconEntry::new("inside", "inside", 90.0),
        ]));
        let candidates = schema.candidates("in");
        assert_eq!(candidates[0].text, "in");
    }

    #[test]
    fn exact_reading_can_output_contraction_surface() {
        let schema = EnglishWordSchema::new(Lexicon::new([
            LexiconEntry::new("didn't", "didnt", 100.0),
            LexiconEntry::new("didn", "didn", 120.0),
        ]));
        let candidates = schema.candidates("didnt");
        assert_eq!(candidates[0].reading, "didnt");
        assert_eq!(candidates[0].text, "didn't");
    }

    #[test]
    fn exact_reading_can_output_possessive_surface() {
        let schema = EnglishWordSchema::new(Lexicon::new([
            LexiconEntry::new("company's", "companys", 100.0),
            LexiconEntry::new("company", "company", 120.0),
        ]));
        let candidates = schema.candidates("companys");
        assert_eq!(candidates[0].reading, "companys");
        assert_eq!(candidates[0].text, "company's");
    }

    #[test]
    fn exact_contraction_beats_high_frequency_deletion() {
        let schema = EnglishWordSchema::new(Lexicon::new([
            LexiconEntry::new("that's", "thats", 100.0),
            LexiconEntry::new("that", "that", 10_000.0),
        ]));
        let candidates = schema.candidates("thats");
        assert_eq!(candidates[0].source, CandidateSource::Exact);
        assert_eq!(candidates[0].text, "that's");
        assert!(candidates.iter().any(|candidate| {
            candidate.source == CandidateSource::Fuzzy && candidate.text == "that"
        }));
    }

    #[test]
    fn rare_exact_typo_loses_to_common_transposition() {
        let schema = EnglishWordSchema::new(Lexicon::new([
            LexiconEntry::new("the", "the", 10_000.0),
            LexiconEntry::new("teh", "teh", 0.0),
        ]));
        let candidates = schema.candidates("teh");
        assert_eq!(candidates[0].source, CandidateSource::Fuzzy);
        assert_eq!(candidates[0].text, "the");
    }

    #[test]
    fn incremental_hello_produces_exact_after_five_advances() {
        let schema = EnglishWordSchema::builtin();
        let mut state = schema.initial_state();
        let mut all_completed = Vec::new();
        for ch in "hello".chars() {
            let results = schema.advance(&state, ch);
            assert!(!results.is_empty());
            all_completed.extend(results[0].completed.clone());
            state = results[0].next_state.clone();
        }
        assert!(all_completed.iter().any(|c| c.text == "hello"));
    }

    #[test]
    fn incremental_candidates_at_returns_prefix_completions() {
        let schema = EnglishWordSchema::builtin();
        let mut state = schema.initial_state();
        for ch in "key".chars() {
            let results = schema.advance(&state, ch);
            state = results[0].next_state.clone();
        }
        let candidates = schema.candidates_at(&state);
        assert!(candidates.iter().any(|c| c.text == "keyboard"));
    }

    #[test]
    fn incremental_reset_arena_clears_state() {
        let schema = EnglishWordSchema::builtin();
        let _ = schema.initial_state();
        assert!(schema.arena.borrow().len() > 0);
        schema.reset_arena();
        assert!(schema.arena.borrow().is_empty());
    }

    #[test]
    fn supported_plural_beats_high_frequency_singular_deletion() {
        let schema = EnglishWordSchema::new(Lexicon::new([
            LexiconEntry::new("time", "time", 10_000.0),
            LexiconEntry::new("times", "times", 100.0),
        ]));
        let candidates = schema.candidates("times");
        assert_eq!(candidates[0].source, CandidateSource::Exact);
        assert_eq!(candidates[0].text, "times");
    }
}
