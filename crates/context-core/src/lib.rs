use std::collections::{HashMap, HashSet};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CommittedText {
    pub text: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TextContext {
    pub committed: CommittedText,
    pub domain: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ContextPredictiveCache {
    reading_bias: HashMap<String, f32>,
    text_bias: HashMap<String, f32>,
    schema_reading_bias: HashMap<String, HashMap<String, f32>>,
    schema_text_bias: HashMap<String, HashMap<String, f32>>,
    continuations_by_suffix: HashMap<String, Vec<ContextualPrediction>>,
    max_suffix_chars: usize,
}

#[derive(Clone, Debug, PartialEq)]
struct ContextualPrediction {
    schema: Option<String>,
    reading: String,
    text: String,
    bonus: f32,
}

pub trait ContextScorer {
    fn score_bonus(&self, reading: &str, text: &str) -> f32;
    fn score_bonus_for_schema(&self, schema: &str, reading: &str, text: &str) -> f32;
}

impl ContextPredictiveCache {
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn add_reading_bias(&mut self, reading: impl Into<String>, bonus: f32) {
        *self.reading_bias.entry(reading.into()).or_insert(0.0) += bonus;
    }

    pub fn add_text_bias(&mut self, text: impl Into<String>, bonus: f32) {
        *self.text_bias.entry(text.into()).or_insert(0.0) += bonus;
    }

    pub fn add_schema_reading_bias(
        &mut self,
        schema: impl Into<String>,
        reading: impl Into<String>,
        bonus: f32,
    ) {
        *self
            .schema_reading_bias
            .entry(schema.into())
            .or_default()
            .entry(reading.into())
            .or_insert(0.0) += bonus;
    }

    pub fn add_schema_text_bias(
        &mut self,
        schema: impl Into<String>,
        text: impl Into<String>,
        bonus: f32,
    ) {
        *self
            .schema_text_bias
            .entry(schema.into())
            .or_default()
            .entry(text.into())
            .or_insert(0.0) += bonus;
    }

    pub fn add_contextual_prediction(
        &mut self,
        suffix: impl Into<String>,
        reading: impl Into<String>,
        text: impl Into<String>,
        bonus: f32,
    ) {
        self.add_contextual_prediction_inner(
            None,
            suffix.into(),
            reading.into(),
            text.into(),
            bonus,
        );
    }

    pub fn add_schema_contextual_prediction(
        &mut self,
        schema: impl Into<String>,
        suffix: impl Into<String>,
        reading: impl Into<String>,
        text: impl Into<String>,
        bonus: f32,
    ) {
        self.add_contextual_prediction_inner(
            Some(schema.into()),
            suffix.into(),
            reading.into(),
            text.into(),
            bonus,
        );
    }

    fn add_contextual_prediction_inner(
        &mut self,
        schema: Option<String>,
        suffix: String,
        reading: String,
        text: String,
        bonus: f32,
    ) {
        if suffix.is_empty() || reading.is_empty() || text.is_empty() || bonus <= 0.0 {
            return;
        }
        self.max_suffix_chars = self.max_suffix_chars.max(suffix.chars().count());
        // Cheap append only. The previous per-insert dedup-find + sort + truncate was
        // O(m·log m) PER call; building the cache from the 200k-entry lexicon (~600k
        // calls) made startup ~2s. Dedup/sort/truncate is deferred to `finalize`,
        // which runs once. To bound transient memory, a suffix vector that grows past
        // a soft cap is compacted early (amortized, rare).
        let predictions = self.continuations_by_suffix.entry(suffix).or_default();
        predictions.push(ContextualPrediction {
            schema,
            reading,
            text,
            bonus,
        });
        if predictions.len() >= 4096 {
            compact_predictions(predictions);
        }
    }

    /// Dedup, rank, and cap every suffix bucket. Call once after bulk-loading
    /// contextual predictions (e.g. from a lexicon) before the cache is queried.
    pub fn finalize(&mut self) {
        for predictions in self.continuations_by_suffix.values_mut() {
            compact_predictions(predictions);
        }
    }

    pub fn contextual_prediction_count(&self) -> usize {
        self.continuations_by_suffix.values().map(Vec::len).sum()
    }

    pub fn merge_from(&mut self, other: &Self) {
        for (reading, bonus) in &other.reading_bias {
            *self.reading_bias.entry(reading.clone()).or_insert(0.0) += bonus;
        }
        for (text, bonus) in &other.text_bias {
            *self.text_bias.entry(text.clone()).or_insert(0.0) += bonus;
        }
        for (schema, readings) in &other.schema_reading_bias {
            for (reading, bonus) in readings {
                self.add_schema_reading_bias(schema.clone(), reading.clone(), *bonus);
            }
        }
        for (schema, texts) in &other.schema_text_bias {
            for (text, bonus) in texts {
                self.add_schema_text_bias(schema.clone(), text.clone(), *bonus);
            }
        }
        for (suffix, predictions) in &other.continuations_by_suffix {
            for prediction in predictions {
                self.add_contextual_prediction_inner(
                    prediction.schema.clone(),
                    suffix.clone(),
                    prediction.reading.clone(),
                    prediction.text.clone(),
                    prediction.bonus,
                );
            }
        }
    }

    pub fn score_bonus(&self, reading: &str, text: &str) -> f32 {
        self.reading_bias.get(reading).copied().unwrap_or(0.0)
            + self.text_bias.get(text).copied().unwrap_or(0.0)
    }

    pub fn score_bonus_for_schema(&self, schema: &str, reading: &str, text: &str) -> f32 {
        self.score_bonus(reading, text)
            + self
                .schema_reading_bias
                .get(schema)
                .and_then(|biases| biases.get(reading))
                .copied()
                .unwrap_or(0.0)
            + self
                .schema_text_bias
                .get(schema)
                .and_then(|biases| biases.get(text))
                .copied()
                .unwrap_or(0.0)
    }
}

impl ContextScorer for ContextPredictiveCache {
    fn score_bonus(&self, reading: &str, text: &str) -> f32 {
        self.score_bonus(reading, text)
    }

    fn score_bonus_for_schema(&self, schema: &str, reading: &str, text: &str) -> f32 {
        self.score_bonus_for_schema(schema, reading, text)
    }
}

#[derive(Debug)]
pub struct CompiledContext<'a> {
    base: &'a ContextPredictiveCache,
    overlay: ContextPredictiveCache,
}

impl ContextScorer for CompiledContext<'_> {
    fn score_bonus(&self, reading: &str, text: &str) -> f32 {
        self.base.score_bonus(reading, text) + self.overlay.score_bonus(reading, text)
    }

    fn score_bonus_for_schema(&self, schema: &str, reading: &str, text: &str) -> f32 {
        self.base.score_bonus_for_schema(schema, reading, text)
            + self.overlay.score_bonus_for_schema(schema, reading, text)
    }
}

#[derive(Clone, Debug, Default)]
pub struct ContextCompiler {
    base_cache: ContextPredictiveCache,
}

impl ContextCompiler {
    pub fn with_base_cache(base_cache: ContextPredictiveCache) -> Self {
        Self { base_cache }
    }

    pub fn compile_view(&self, context: &TextContext) -> CompiledContext<'_> {
        let mut overlay = ContextPredictiveCache::empty();
        for suffix in context_suffixes(&context.committed.text, self.base_cache.max_suffix_chars) {
            let Some(predictions) = self.base_cache.continuations_by_suffix.get(&suffix) else {
                continue;
            };
            for prediction in predictions {
                if let Some(schema) = prediction.schema.as_ref() {
                    overlay.add_schema_reading_bias(
                        schema,
                        &prediction.reading,
                        prediction.bonus * 0.5,
                    );
                    overlay.add_schema_text_bias(schema, &prediction.text, prediction.bonus * 0.5);
                } else {
                    overlay.add_reading_bias(&prediction.reading, prediction.bonus * 0.5);
                    overlay.add_text_bias(&prediction.text, prediction.bonus * 0.5);
                }
            }
        }
        CompiledContext {
            base: &self.base_cache,
            overlay,
        }
    }

    pub fn compile(&self, context: &TextContext) -> ContextPredictiveCache {
        let mut cache = self.base_cache.clone();
        cache.merge_from(&self.compile_view(context).overlay);
        cache
    }
}

fn compact_predictions(predictions: &mut Vec<ContextualPrediction>) {
    // Group identical (schema, reading, text) and merge them to the max bonus.
    predictions.sort_by(|a, b| {
        a.schema
            .cmp(&b.schema)
            .then_with(|| a.reading.cmp(&b.reading))
            .then_with(|| a.text.cmp(&b.text))
    });
    predictions.dedup_by(|a, b| {
        if a.schema == b.schema && a.reading == b.reading && a.text == b.text {
            b.bonus = b.bonus.max(a.bonus);
            true
        } else {
            false
        }
    });
    // Rank by bonus and keep the best 512 (matches the previous per-insert cap).
    predictions.sort_by(|a, b| {
        b.bonus
            .total_cmp(&a.bonus)
            .then_with(|| a.text.cmp(&b.text))
            .then_with(|| a.reading.cmp(&b.reading))
    });
    predictions.truncate(512);
}

fn context_suffixes(text: &str, max_chars: usize) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut suffixes = Vec::new();
    for suffix in suffixes_up_to(text, max_chars)
        .into_iter()
        .chain(ascii_word_suffixes_up_to(text, max_chars))
    {
        if seen.insert(suffix.clone()) {
            suffixes.push(suffix);
        }
    }
    suffixes
}

fn suffixes_up_to(text: &str, max_chars: usize) -> Vec<String> {
    if max_chars == 0 || text.is_empty() {
        return Vec::new();
    }
    let chars = text.chars().collect::<Vec<_>>();
    let start = chars.len().saturating_sub(max_chars);
    (start..chars.len())
        .map(|idx| chars[idx..].iter().collect())
        .collect()
}

fn ascii_word_suffixes_up_to(text: &str, max_chars: usize) -> Vec<String> {
    if max_chars == 0 {
        return Vec::new();
    }
    let words = text
        .split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '\''))
        .filter(|word| !word.is_empty())
        .map(|word| word.to_ascii_lowercase())
        .collect::<Vec<_>>();
    let mut suffixes = Vec::new();
    for start in 0..words.len() {
        let suffix = words[start..].join(" ");
        if suffix.chars().count() <= max_chars {
            suffixes.push(suffix);
        }
    }
    suffixes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compiler_includes_base_priors() {
        let mut base = ContextPredictiveCache::empty();
        base.add_reading_bias("hello", 0.25);
        let compiler = ContextCompiler::with_base_cache(base);
        let cache = compiler.compile(&TextContext::default());
        assert_eq!(cache.score_bonus("hello", ""), 0.25);
    }

    #[test]
    fn committed_context_merges_with_base_priors() {
        let mut base = ContextPredictiveCache::empty();
        base.add_reading_bias("xi an", 0.5);
        base.add_contextual_prediction("去", "xi an", "西安", 3.0);
        let compiler = ContextCompiler::with_base_cache(base);
        let cache = compiler.compile(&TextContext {
            committed: CommittedText {
                text: "我想去".to_owned(),
            },
            domain: None,
        });
        assert_eq!(cache.score_bonus("xi an", "西安"), 3.5);
    }

    #[test]
    fn contextual_predictions_match_unicode_suffixes() {
        let mut base = ContextPredictiveCache::empty();
        base.add_contextual_prediction("学校", "men kou", "门口", 2.0);
        let compiler = ContextCompiler::with_base_cache(base);
        let cache = compiler.compile(&TextContext {
            committed: CommittedText {
                text: "我到学校".to_owned(),
            },
            domain: None,
        });
        assert_eq!(cache.score_bonus("men kou", "门口"), 2.0);
    }

    #[test]
    fn contextual_predictions_match_english_word_suffixes() {
        let mut base = ContextPredictiveCache::empty();
        base.add_schema_contextual_prediction("en-word", "i", "cannot", "cannot", 2.0);
        let compiler = ContextCompiler::with_base_cache(base);
        let cache = compiler.compile(&TextContext {
            committed: CommittedText {
                text: "I ".to_owned(),
            },
            domain: None,
        });
        assert_eq!(
            cache.score_bonus_for_schema("en-word", "cannot", "cannot"),
            2.0
        );
    }

    #[test]
    fn contextual_predictions_match_multi_word_english_suffixes() {
        let mut base = ContextPredictiveCache::empty();
        base.add_schema_contextual_prediction("en-word", "i really", "cannot", "cannot", 2.0);
        let compiler = ContextCompiler::with_base_cache(base);
        let cache = compiler.compile(&TextContext {
            committed: CommittedText {
                text: "Well, I really ".to_owned(),
            },
            domain: None,
        });
        assert_eq!(
            cache.score_bonus_for_schema("en-word", "cannot", "cannot"),
            2.0
        );
    }
}
