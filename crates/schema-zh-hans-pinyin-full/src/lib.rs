mod abbrev;
mod syllable_trie;

use std::cell::{OnceCell, RefCell};
use std::collections::{HashMap, HashSet};

use data_core::{Lexicon, LexiconEntry, WeightedReading, normalize_reading};
use schema_core::{
    Boundary, CandidateSource, IncrementalSchema, ReadingCandidate, Schema, SchemaAdvanceResult,
    SchemaId, SchemaStateId,
};
use syllable_trie::{SyllableTrieNode, build_syllable_trie};

const MAX_SEGMENTATIONS: usize = 64;
const MAX_SEGMENT_SYLLABLES: usize = 8;

#[derive(Clone, Debug)]
struct PinyinInternalState {
    trie_position: usize,
    completed_syllables: Vec<String>,
}

/// Rime-style credibility ladder, expressed as additive costs (we minimize cost).
/// Full pinyin = 0; 简拼 (abbreviation by syllable initials) ≈ -log(0.1);
/// 补全 (completion of an unfinished trailing syllable) ≈ -log(0.05).
const ABBREV_PENALTY: f32 = 2.3;
const COMPLETION_PENALTY: f32 = 3.0;
/// Max initials kept per abbreviation key, and the syllable-count window we index.
const ABBREV_PER_KEY: usize = 32;
const ABBREV_MIN_SYLLABLES: usize = 2;
const ABBREV_MAX_SYLLABLES: usize = 6;

#[derive(Clone, Debug)]
pub struct ZhHansPinyinFullSchema {
    syllable_trie: Vec<SyllableTrieNode>,
    lexicon: Lexicon,
    compact_reading_costs: HashMap<String, HashMap<String, f32>>,
    arena: RefCell<Vec<PinyinInternalState>>,
    /// Lazily built map: syllable-initials (e.g. "dx" for 东西/"dong xi") -> lexicon
    /// entry indices, sorted by entry_cost (most frequent first), capped per key.
    abbrev_index: OnceCell<HashMap<String, Vec<u32>>>,
}

impl ZhHansPinyinFullSchema {
    pub fn new(lexicon: Lexicon) -> Self {
        Self::with_syllables(lexicon, common_syllables().into_iter().map(str::to_owned))
    }

    pub fn with_syllables<I, S>(lexicon: Lexicon, syllables: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self::with_syllables_and_ambiguous_readings(lexicon, syllables, HashMap::new())
    }

    pub fn with_syllables_and_ambiguous_readings<I, S>(
        lexicon: Lexicon,
        syllables: I,
        ambiguous_compact_readings: HashMap<String, Vec<WeightedReading>>,
    ) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let syllables = syllables
            .into_iter()
            .map(|syllable| normalize_reading(&syllable.into()))
            .collect::<HashSet<_>>();
        let syllable_trie = build_syllable_trie(&syllables);
        Self {
            syllable_trie,
            lexicon,
            compact_reading_costs: compact_reading_costs(ambiguous_compact_readings),
            arena: RefCell::new(Vec::new()),
            abbrev_index: OnceCell::new(),
        }
    }

    pub fn builtin() -> Self {
        Self::new(Lexicon::new([
            LexiconEntry::new("你好", "ni hao", 1200.0),
            LexiconEntry::new("西安", "xi an", 1100.0),
            LexiconEntry::new("先", "xian", 800.0),
            LexiconEntry::new("中国", "zhong guo", 1000.0),
            LexiconEntry::new("学校", "xue xiao", 700.0),
            LexiconEntry::new("北京", "bei jing", 900.0),
        ]))
    }

    fn segment(&self, symbols: &str) -> Vec<Vec<String>> {
        let mut out = Vec::new();
        self.segment_from(symbols, 0, &mut Vec::new(), &mut out);
        out
    }

    fn segment_from(
        &self,
        symbols: &str,
        start: usize,
        current: &mut Vec<String>,
        out: &mut Vec<Vec<String>>,
    ) {
        if out.len() >= MAX_SEGMENTATIONS || current.len() >= MAX_SEGMENT_SYLLABLES {
            return;
        }
        if start == symbols.len() {
            out.push(current.clone());
            return;
        }
        for (end, syllable) in self.match_syllables_at(symbols, start) {
            if out.len() >= MAX_SEGMENTATIONS {
                return;
            }
            current.push(syllable);
            self.segment_from(symbols, end, current, out);
            current.pop();
        }
    }

    fn match_syllables_at(&self, symbols: &str, start: usize) -> Vec<(usize, String)> {
        let mut matches = Vec::new();
        let mut node_index = 0usize;
        for (offset, ch) in symbols[start..].char_indices() {
            let Some(next) = self.syllable_trie[node_index].children.get(&ch) else {
                break;
            };
            node_index = *next;
            if let Some(syllable) = self.syllable_trie[node_index].syllable.as_ref() {
                matches.push((start + offset + ch.len_utf8(), syllable.clone()));
            }
        }
        matches
    }

    fn syllable_prefix_completions(&self, prefix: &str, limit: usize) -> Vec<String> {
        if prefix.is_empty() || limit == 0 {
            return Vec::new();
        }
        let mut node_index = 0usize;
        for ch in prefix.chars() {
            let Some(next) = self.syllable_trie[node_index].children.get(&ch) else {
                return Vec::new();
            };
            node_index = *next;
        }

        let mut out = Vec::new();
        self.collect_syllables(node_index, limit, &mut out);
        out.sort_by(|a, b| a.len().cmp(&b.len()).then_with(|| a.cmp(b)));
        out.truncate(limit);
        out
    }

    fn collect_syllables(&self, node_index: usize, limit: usize, out: &mut Vec<String>) {
        if out.len() >= limit {
            return;
        }
        if let Some(syllable) = self.syllable_trie[node_index].syllable.as_ref() {
            out.push(syllable.clone());
            if out.len() >= limit {
                return;
            }
        }
        let mut children = self.syllable_trie[node_index]
            .children
            .iter()
            .collect::<Vec<_>>();
        children.sort_by_key(|(ch, _)| **ch);
        for (_, child) in children {
            self.collect_syllables(*child, limit, out);
            if out.len() >= limit {
                return;
            }
        }
    }
}

impl Default for ZhHansPinyinFullSchema {
    fn default() -> Self {
        Self::builtin()
    }
}

impl Schema for ZhHansPinyinFullSchema {
    fn id(&self) -> SchemaId {
        SchemaId::new("zh-hans-pinyin-full")
    }

    fn candidates(&self, symbol_path: &str) -> Vec<ReadingCandidate> {
        let normalized = normalize_reading(symbol_path).replace(' ', "");
        let mut candidates = Vec::new();
        for segments in self.segment(&normalized) {
            let reading = segments.join(" ");
            let boundary = Boundary {
                segments: segments.clone(),
            };
            let mut matched_exact = false;
            for entry in &self.lexicon.lookup_reading(&reading) {
                matched_exact = true;
                candidates.push(ReadingCandidate {
                    reading: reading.clone(),
                    boundary: boundary.clone(),
                    text: entry.text.clone(),
                    cost: self.lexicon.entry_cost(entry)
                        + self.reading_prior_cost(&normalized, &reading),
                    source: CandidateSource::Exact,
                });
            }
            if !matched_exact {
                candidates.push(ReadingCandidate {
                    reading: reading.clone(),
                    boundary,
                    text: reading,
                    cost: 8.0 + segments.len() as f32 * 0.25,
                    source: CandidateSource::Raw,
                });
            }
        }
        if candidates.is_empty() {
            // 简拼 (initials, e.g. dx -> 东西) and 补全 (unfinished trailing syllable,
            // e.g. zhesh -> 这是). Both are scored below full pinyin via penalties.
            candidates.extend(self.abbreviation_candidates(&normalized));
            candidates.extend(self.prefix_completion_candidates(&normalized));
            candidates.extend(self.partial_final_syllable_candidates(&normalized));
        }
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

impl ZhHansPinyinFullSchema {
    fn alloc_state(&self, state: PinyinInternalState) -> u32 {
        let mut arena = self.arena.borrow_mut();
        let index = arena.len() as u32;
        arena.push(state);
        index
    }

    fn get_state(&self, index: u32) -> PinyinInternalState {
        self.arena.borrow()[index as usize].clone()
    }

    fn lookup_completed(&self, syllables: &[String], compact: &str) -> Vec<ReadingCandidate> {
        if syllables.is_empty() {
            return Vec::new();
        }
        let reading = syllables.join(" ");
        let mut candidates = Vec::new();
        for entry in &self.lexicon.lookup_reading(&reading) {
            candidates.push(ReadingCandidate {
                reading: reading.clone(),
                boundary: Boundary {
                    segments: syllables.to_vec(),
                },
                text: entry.text.clone(),
                cost: self.lexicon.entry_cost(entry) + self.reading_prior_cost(compact, &reading),
                source: CandidateSource::Exact,
            });
        }
        candidates
    }

}

impl IncrementalSchema for ZhHansPinyinFullSchema {
    fn initial_state(&self) -> SchemaStateId {
        let index = self.alloc_state(PinyinInternalState {
            trie_position: 0,
            completed_syllables: Vec::new(),
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
        let mut results = Vec::new();

        // Path A: extend current partial syllable in the trie
        if let Some(&next_pos) = self.syllable_trie[internal.trie_position]
            .children
            .get(&symbol)
        {
            let mut completed = Vec::new();
            if let Some(syllable) = &self.syllable_trie[next_pos].syllable {
                // This position completes a syllable — look up the accumulated reading
                let mut syls = internal.completed_syllables.clone();
                syls.push(syllable.clone());
                let compact: String = syls.iter().map(|s| s.as_str()).collect();
                completed = self.lookup_completed(&syls, &compact);
            }
            let has_children = !self.syllable_trie[next_pos].children.is_empty();
            let alive = has_children || self.syllable_trie[next_pos].syllable.is_some();
            let new_index = self.alloc_state(PinyinInternalState {
                trie_position: next_pos,
                completed_syllables: internal.completed_syllables.clone(),
            });
            results.push(SchemaAdvanceResult {
                next_state: SchemaStateId {
                    state_index: new_index,
                    accumulated_cost: state.accumulated_cost,
                    symbol_count: state.symbol_count + 1,
                    alive,
                },
                cost_delta: 0.0,
                completed,
            });
        }

        // Path B: if current position completes a syllable, commit it and start
        // a new syllable from root with this symbol
        if let Some(syllable) = &self.syllable_trie[internal.trie_position].syllable {
            if let Some(&new_pos) = self.syllable_trie[0].children.get(&symbol) {
                let mut new_syllables = internal.completed_syllables.clone();
                new_syllables.push(syllable.clone());

                let mut completed = Vec::new();
                if let Some(new_syl) = &self.syllable_trie[new_pos].syllable {
                    // The new char also immediately completes a syllable (e.g., 'a', 'e', 'o')
                    let mut syls = new_syllables.clone();
                    syls.push(new_syl.clone());
                    let compact: String = syls.iter().map(|s| s.as_str()).collect();
                    completed = self.lookup_completed(&syls, &compact);
                }

                let has_children = !self.syllable_trie[new_pos].children.is_empty();
                let alive = has_children || self.syllable_trie[new_pos].syllable.is_some();
                let new_index = self.alloc_state(PinyinInternalState {
                    trie_position: new_pos,
                    completed_syllables: new_syllables,
                });
                results.push(SchemaAdvanceResult {
                    next_state: SchemaStateId {
                        state_index: new_index,
                        accumulated_cost: state.accumulated_cost,
                        symbol_count: state.symbol_count + 1,
                        alive,
                    },
                    cost_delta: 0.0,
                    completed,
                });
            }
        }

        if results.is_empty() && internal.completed_syllables.is_empty() {
            // At root with no trie match — produce raw fallback
            let text = symbol.to_string();
            let new_index = self.alloc_state(PinyinInternalState {
                trie_position: 0,
                completed_syllables: Vec::new(),
            });
            results.push(SchemaAdvanceResult {
                next_state: SchemaStateId {
                    state_index: new_index,
                    accumulated_cost: state.accumulated_cost + 8.0,
                    symbol_count: state.symbol_count + 1,
                    alive: false,
                },
                cost_delta: 8.0,
                completed: vec![ReadingCandidate {
                    reading: text.clone(),
                    boundary: Boundary::from_reading(&text),
                    text,
                    cost: 8.0,
                    source: CandidateSource::Raw,
                }],
            });
        }

        results
    }

    fn candidates_at(&self, state: &SchemaStateId) -> Vec<ReadingCandidate> {
        let internal = self.get_state(state.state_index);
        let mut candidates = Vec::new();

        // If at a syllable boundary, return lexicon matches
        if let Some(syllable) = &self.syllable_trie[internal.trie_position].syllable {
            let mut syls = internal.completed_syllables.clone();
            syls.push(syllable.clone());
            let compact: String = syls.iter().map(|s| s.as_str()).collect();
            candidates.extend(self.lookup_completed(&syls, &compact));
        }

        // Also return candidates for already-completed syllables (without current partial)
        if !internal.completed_syllables.is_empty() {
            let compact: String = internal
                .completed_syllables
                .iter()
                .map(|s| s.as_str())
                .collect();
            candidates.extend(self.lookup_completed(&internal.completed_syllables, &compact));
        }

        candidates.sort_by(|a, b| a.cost.total_cmp(&b.cost));
        candidates.dedup_by(|a, b| a.text == b.text && a.reading == b.reading);
        candidates
    }

    fn reset_arena(&self) {
        self.arena.borrow_mut().clear();
    }
}

impl ZhHansPinyinFullSchema {
    fn prefix_completion_candidates(&self, normalized: &str) -> Vec<ReadingCandidate> {
        let mut candidates = Vec::new();
        for syllable in self.syllable_prefix_completions(normalized, 24) {
            let missing_chars = syllable
                .chars()
                .count()
                .saturating_sub(normalized.chars().count());
            for entry in &self.lexicon.lookup_reading_prefix(&syllable, 12) {
                candidates.push(ReadingCandidate {
                    reading: entry.reading.clone(),
                    boundary: Boundary::from_reading(&entry.reading),
                    text: entry.text.clone(),
                    cost: self.lexicon.entry_cost(entry)
                        + COMPLETION_PENALTY
                        + 1.5
                        + missing_chars as f32 * 0.2,
                    source: CandidateSource::Prefix,
                });
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

    fn partial_final_syllable_candidates(&self, normalized: &str) -> Vec<ReadingCandidate> {
        let mut candidates = Vec::new();
        self.partial_final_syllable_from(normalized, 0, &mut Vec::new(), &mut candidates);
        candidates.sort_by(|a, b| {
            a.cost
                .total_cmp(&b.cost)
                .then_with(|| a.text.cmp(&b.text))
                .then_with(|| a.reading.cmp(&b.reading))
        });
        candidates.dedup_by(|a, b| a.text == b.text && a.reading == b.reading);
        candidates.truncate(24);
        candidates
    }

    fn partial_final_syllable_from(
        &self,
        symbols: &str,
        start: usize,
        current: &mut Vec<String>,
        out: &mut Vec<ReadingCandidate>,
    ) {
        if start >= symbols.len()
            || out.len() >= 128
            || current.len() >= MAX_SEGMENT_SYLLABLES.saturating_sub(1)
        {
            return;
        }

        let suffix = &symbols[start..];
        if !current.is_empty() {
            for completed in self.syllable_prefix_completions(suffix, 16) {
                if completed == suffix {
                    continue;
                }
                let mut reading_segments = current.clone();
                reading_segments.push(completed.clone());
                let reading_prefix = reading_segments.join(" ");
                let missing_chars = completed
                    .chars()
                    .count()
                    .saturating_sub(suffix.chars().count());
                for entry in &self.lexicon.lookup_reading_prefix(&reading_prefix, 8) {
                    out.push(ReadingCandidate {
                        reading: entry.reading.clone(),
                        boundary: Boundary::from_reading(&entry.reading),
                        text: entry.text.clone(),
                        cost: self.lexicon.entry_cost(entry)
                            + COMPLETION_PENALTY
                            + missing_chars as f32 * 0.2
                            + current.len() as f32 * 0.08,
                        source: CandidateSource::Prefix,
                    });
                }
            }
        }

        for (end, syllable) in self.match_syllables_at(symbols, start) {
            if end >= symbols.len() {
                continue;
            }
            current.push(syllable);
            self.partial_final_syllable_from(symbols, end, current, out);
            current.pop();
        }
    }

    fn reading_prior_cost(&self, compact: &str, reading: &str) -> f32 {
        self.compact_reading_costs
            .get(compact)
            .and_then(|readings| readings.get(reading))
            .copied()
            .unwrap_or(0.0)
    }
}

fn compact_reading_costs(
    ambiguous_compact_readings: HashMap<String, Vec<WeightedReading>>,
) -> HashMap<String, HashMap<String, f32>> {
    ambiguous_compact_readings
        .into_iter()
        .map(|(compact, readings)| {
            let best = readings
                .iter()
                .map(|reading| reading.weight)
                .fold(0.0f32, f32::max);
            let costs = readings
                .into_iter()
                .map(|reading| {
                    let cost = if best > 0.0 && reading.weight > 0.0 {
                        ((best + 1.0) / (reading.weight + 1.0)).ln()
                    } else {
                        0.0
                    };
                    (normalize_reading(&reading.reading), cost)
                })
                .collect::<HashMap<_, _>>();
            (compact, costs)
        })
        .collect()
}

fn common_syllables() -> HashSet<&'static str> {
    [
        "a", "ai", "an", "ang", "ao", "ba", "bai", "ban", "bang", "bao", "bei", "ben", "beng",
        "bi", "bian", "biao", "bie", "bin", "bing", "bo", "bu", "ca", "cai", "can", "cang", "cao",
        "ce", "cen", "ceng", "cha", "chai", "chan", "chang", "chao", "che", "chen", "cheng", "chi",
        "chong", "chou", "chu", "chuai", "chuan", "chuang", "chui", "chun", "chuo", "ci", "cong",
        "cou", "cu", "cuan", "cui", "cun", "cuo", "da", "dai", "dan", "dang", "dao", "de", "dei",
        "den", "deng", "di", "dia", "dian", "diao", "die", "ding", "diu", "dong", "dou", "du",
        "duan", "dui", "dun", "duo", "e", "ei", "en", "eng", "er", "fa", "fan", "fang", "fei",
        "fen", "feng", "fo", "fou", "fu", "ga", "gai", "gan", "gang", "gao", "ge", "gei", "gen",
        "geng", "gong", "gou", "gu", "gua", "guai", "guan", "guang", "gui", "gun", "guo", "ha",
        "hai", "han", "hang", "hao", "he", "hei", "hen", "heng", "hong", "hou", "hu", "hua",
        "huai", "huan", "huang", "hui", "hun", "huo", "ji", "jia", "jian", "jiang", "jiao", "jie",
        "jin", "jing", "jiong", "jiu", "ju", "juan", "jue", "jun", "ka", "kai", "kan", "kang",
        "kao", "ke", "ken", "keng", "kong", "kou", "ku", "kua", "kuai", "kuan", "kuang", "kui",
        "kun", "kuo", "la", "lai", "lan", "lang", "lao", "le", "lei", "leng", "li", "lia", "lian",
        "liang", "liao", "lie", "lin", "ling", "liu", "lo", "long", "lou", "lu", "luan", "lun",
        "luo", "lv", "lve", "ma", "mai", "man", "mang", "mao", "me", "mei", "men", "meng", "mi",
        "mian", "miao", "mie", "min", "ming", "miu", "mo", "mou", "mu", "na", "nai", "nan", "nang",
        "nao", "ne", "nei", "nen", "neng", "ni", "nian", "niang", "niao", "nie", "nin", "ning",
        "niu", "nong", "nou", "nu", "nuan", "nun", "nuo", "nv", "nve", "o", "ou", "pa", "pai",
        "pan", "pang", "pao", "pei", "pen", "peng", "pi", "pian", "piao", "pie", "pin", "ping",
        "po", "pou", "pu", "qi", "qia", "qian", "qiang", "qiao", "qie", "qin", "qing", "qiong",
        "qiu", "qu", "quan", "que", "qun", "ran", "rang", "rao", "re", "ren", "reng", "ri", "rong",
        "rou", "ru", "ruan", "rui", "run", "ruo", "sa", "sai", "san", "sang", "sao", "se", "sen",
        "seng", "sha", "shai", "shan", "shang", "shao", "she", "shen", "sheng", "shi", "shou",
        "shu", "shua", "shuai", "shuan", "shuang", "shui", "shun", "shuo", "si", "song", "sou",
        "su", "suan", "sui", "sun", "suo", "ta", "tai", "tan", "tang", "tao", "te", "teng", "ti",
        "tian", "tiao", "tie", "ting", "tong", "tou", "tu", "tuan", "tui", "tun", "tuo", "wa",
        "wai", "wan", "wang", "wei", "wen", "weng", "wo", "wu", "xi", "xia", "xian", "xiang",
        "xiao", "xie", "xin", "xing", "xiong", "xiu", "xu", "xuan", "xue", "xun", "ya", "yan",
        "yang", "yao", "ye", "yi", "yin", "ying", "yo", "yong", "you", "yu", "yuan", "yue", "yun",
        "za", "zai", "zan", "zang", "zao", "ze", "zei", "zen", "zeng", "zha", "zhai", "zhan",
        "zhang", "zhao", "zhe", "zhen", "zheng", "zhi", "zhong", "zhou", "zhu", "zhua", "zhuai",
        "zhuan", "zhuang", "zhui", "zhun", "zhuo", "zi", "zong", "zou", "zu", "zuan", "zui", "zun",
        "zuo",
    ]
    .into_iter()
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_xian_and_xi_an() {
        let schema = ZhHansPinyinFullSchema::builtin();
        let candidates = schema.candidates("xian");
        assert!(
            candidates
                .iter()
                .any(|candidate| candidate.reading == "xian")
        );
        assert!(
            candidates
                .iter()
                .any(|candidate| candidate.reading == "xi an")
        );
    }

    #[test]
    fn frequency_cost_orders_candidates_inside_reading() {
        let schema = ZhHansPinyinFullSchema::new(Lexicon::new([
            LexiconEntry::new("低频", "ni", 1.0),
            LexiconEntry::new("高频", "ni", 100.0),
        ]));
        let candidates = schema.candidates("ni");
        assert_eq!(candidates[0].text, "高频");
        assert!(candidates[0].cost < candidates[1].cost);
    }

    #[test]
    fn exact_reading_suppresses_raw_pinyin_fallback() {
        let schema =
            ZhHansPinyinFullSchema::new(Lexicon::new([LexiconEntry::new("我", "wo", 1.0)]));
        let candidates = schema.candidates("wo");
        assert_eq!(candidates[0].text, "我");
        assert!(
            candidates
                .iter()
                .all(|candidate| candidate.source != CandidateSource::Raw)
        );
    }

    #[test]
    fn abbreviation_initials_produce_word() {
        // 简拼: dx -> 东西 (dong xi); higher-frequency word ranks first.
        let schema = ZhHansPinyinFullSchema::with_syllables(
            Lexicon::new([
                LexiconEntry::new("东西", "dong xi", 100.0),
                LexiconEntry::new("大学", "da xue", 50.0),
            ]),
            ["dong", "xi", "da", "xue"],
        );
        let candidates = schema.candidates("dx");
        assert_eq!(candidates[0].text, "东西");
        // No full segmentation exists for "dx", so the match must be the abbreviation path.
        assert!(candidates.iter().any(|c| c.text == "大学"));
    }

    #[test]
    fn completion_of_unfinished_final_syllable() {
        // 补全: zhesh -> 这是 ("zhe" complete + "sh" completes to "shi").
        let schema = ZhHansPinyinFullSchema::with_syllables(
            Lexicon::new([LexiconEntry::new("这是", "zhe shi", 100.0)]),
            ["zhe", "shi", "shu", "she"],
        );
        let candidates = schema.candidates("zhesh");
        assert!(candidates.iter().any(|c| c.text == "这是"));
    }

    #[test]
    fn compact_reading_prior_penalizes_weaker_segmentation() {
        let schema = ZhHansPinyinFullSchema::with_syllables_and_ambiguous_readings(
            Lexicon::new([
                LexiconEntry::new("单音", "xian", 100.0),
                LexiconEntry::new("双音", "xi an", 100.0),
            ]),
            ["xi", "an", "xian"],
            HashMap::from([(
                "xian".to_owned(),
                vec![
                    WeightedReading {
                        reading: "xian".to_owned(),
                        weight: 10.0,
                    },
                    WeightedReading {
                        reading: "xi an".to_owned(),
                        weight: 100.0,
                    },
                ],
            )]),
        );
        let candidates = schema.candidates("xian");
        assert_eq!(candidates[0].text, "双音");
    }

    #[test]
    fn streams_predictions_for_incomplete_syllable_prefix() {
        let schema = ZhHansPinyinFullSchema::new(Lexicon::new([
            LexiconEntry::new("西安", "xi an", 100.0),
            LexiconEntry::new("喜欢", "xi huan", 90.0),
            LexiconEntry::new("先", "xian", 80.0),
        ]));
        let candidates = schema.candidates("x");
        assert!(
            candidates
                .iter()
                .any(|candidate| candidate.text == "西安" && candidate.reading == "xi an")
        );
        assert!(
            candidates
                .iter()
                .any(|candidate| candidate.text == "先" && candidate.reading == "xian")
        );
    }

    #[test]
    fn streams_predictions_when_final_syllable_is_incomplete() {
        let schema = ZhHansPinyinFullSchema::new(Lexicon::new([
            LexiconEntry::new("西安门", "xi an men", 100.0),
            LexiconEntry::new("先民", "xian min", 90.0),
            LexiconEntry::new("西安", "xi an", 80.0),
        ]));
        let candidates = schema.candidates("xianm");
        assert!(candidates.iter().any(|candidate| {
            candidate.text == "西安门" && candidate.reading == "xi an men"
        }));
        assert!(
            candidates
                .iter()
                .any(|candidate| candidate.text == "先民" && candidate.reading == "xian min")
        );
    }

    #[test]
    fn incremental_nihao_produces_candidates_after_five_advances() {
        let schema = ZhHansPinyinFullSchema::builtin();
        schema.reset_arena();
        let mut state = schema.initial_state();
        let mut all_completed = Vec::new();
        for ch in "nihao".chars() {
            let results = schema.advance(&state, ch);
            assert!(
                !results.is_empty(),
                "advance for '{ch}' produced no results"
            );
            let best = results
                .into_iter()
                .min_by(|a, b| {
                    a.next_state
                        .accumulated_cost
                        .total_cmp(&b.next_state.accumulated_cost)
                })
                .unwrap();
            all_completed.extend(best.completed.clone());
            state = best.next_state;
        }
        let at_end = schema.candidates_at(&state);
        all_completed.extend(at_end);
        assert!(
            all_completed.iter().any(|c| c.text == "你好"),
            "Expected '你好' in candidates, got: {:?}",
            all_completed.iter().map(|c| &c.text).collect::<Vec<_>>()
        );
    }

    #[test]
    fn incremental_xian_branches_into_two_segmentations() {
        let schema = ZhHansPinyinFullSchema::builtin();
        schema.reset_arena();
        let init = schema.initial_state();

        // Advance through x-i-a-n
        let after_x = schema.advance(&init, 'x');
        assert!(!after_x.is_empty());

        let after_xi = schema.advance(&after_x[0].next_state, 'i');
        assert!(!after_xi.is_empty());

        let after_xia = schema.advance(&after_xi[0].next_state, 'a');
        // At "xia", we should have Path A (extend to "xia" in trie) and potentially
        // Path B (commit "xi", start "a")
        assert!(
            after_xia.len() >= 2,
            "Expected at least 2 branches at 'xia', got {}",
            after_xia.len()
        );

        // Advance the "extend" path to 'n' → should reach "xian"
        let extend_path = &after_xia[0];
        let after_xian = schema.advance(&extend_path.next_state, 'n');
        assert!(!after_xian.is_empty());
        // "xian" should produce candidates for reading "xian" (e.g., "先")
        let xian_candidates: Vec<_> = after_xian.iter().flat_map(|r| r.completed.iter()).collect();
        assert!(
            xian_candidates.iter().any(|c| c.text == "先"),
            "Expected '先' from xian path, got: {:?}",
            xian_candidates.iter().map(|c| &c.text).collect::<Vec<_>>()
        );

        // The "commit xi, start a" branch → advance with 'n' → should reach "an"
        let commit_path = &after_xia[1];
        let after_xi_an = schema.advance(&commit_path.next_state, 'n');
        let xi_an_candidates: Vec<_> = after_xi_an
            .iter()
            .flat_map(|r| r.completed.iter())
            .collect();
        assert!(
            xi_an_candidates.iter().any(|c| c.text == "西安"),
            "Expected '西安' from xi+an path, got: {:?}",
            xi_an_candidates.iter().map(|c| &c.text).collect::<Vec<_>>()
        );
    }

    #[test]
    fn incremental_reset_arena_clears_state() {
        let schema = ZhHansPinyinFullSchema::builtin();
        schema.reset_arena();
        let _ = schema.initial_state();
        let _ = schema.initial_state();
        assert!(schema.arena.borrow().len() == 2);
        schema.reset_arena();
        assert!(schema.arena.borrow().is_empty());
    }
}
