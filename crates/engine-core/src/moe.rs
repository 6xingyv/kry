//! MoE multilingual decoding: per-language **Expert** backends, a **Router** that
//! gates the input to the right language(s), and a **Coordinator** that merges the
//! activated experts' candidates with a language-switch penalty.
//!
//! Principle: every language only owns its own backend — an expert never scores
//! another language's candidates. See `docs/moe-architecture.md`.

use schema_core::{CandidateSource, ReadingCandidate, Schema};

/// A per-language backend. Owns its schema(s)/lexicon (and, later, its own LM) and
/// scores only its own language's candidates.
pub trait LanguageExpert {
    fn lang(&self) -> &str;
    /// Router signal in `0.0..=1.0`: confidence that `symbol_path` is this language
    /// (fraction of real, non-raw parses the schema yields).
    fn schema_fit(&self, symbol_path: &str) -> f32;
    /// Self-scored candidates for this language (raw passthrough excluded).
    fn candidates(&self, symbol_path: &str) -> Vec<ReadingCandidate>;
}

/// Expert backed by an existing [`Schema`]. Borrows the schema so the engine can
/// build experts from the schemas it already owns.
pub struct SchemaExpert<'a> {
    lang: String,
    schema: &'a dyn Schema,
}

impl<'a> SchemaExpert<'a> {
    pub fn new(lang: impl Into<String>, schema: &'a dyn Schema) -> Self {
        Self { lang: lang.into(), schema }
    }
}

impl LanguageExpert for SchemaExpert<'_> {
    fn lang(&self) -> &str {
        &self.lang
    }

    fn schema_fit(&self, symbol_path: &str) -> f32 {
        let cands = self.schema.candidates(symbol_path);
        if cands.is_empty() {
            return 0.0;
        }
        let real = cands
            .iter()
            .filter(|c| c.source != CandidateSource::Raw)
            .count();
        real as f32 / cands.len() as f32
    }

    fn candidates(&self, symbol_path: &str) -> Vec<ReadingCandidate> {
        self.schema
            .candidates(symbol_path)
            .into_iter()
            .filter(|c| c.source != CandidateSource::Raw)
            .collect()
    }
}

/// Heuristic language router (v1). Produces a gate **cost** per expert
/// (lower = more likely) from schema-fit, profile prior, and context stickiness.
pub struct Router {
    /// Weight on poor schema fit. The dominant signal — a path that doesn't parse
    /// as a language is almost certainly not that language.
    pub fit_weight: f32,
    /// Weight on the profile prior (e.g. en-word costs more in a Chinese profile).
    pub profile_weight: f32,
    /// Cost of switching away from the last committed language (stickiness).
    pub context_weight: f32,
}

impl Default for Router {
    fn default() -> Self {
        Self {
            fit_weight: 6.0,
            profile_weight: 1.0,
            context_weight: 2.0,
        }
    }
}

impl Router {
    /// Gate cost per expert (lower = more likely). `profile_prior[i]` is expert i's
    /// profile cost; `context_lang` is the last committed language, if any.
    pub fn gate(
        &self,
        experts: &[&dyn LanguageExpert],
        symbol_path: &str,
        profile_prior: &[f32],
        context_lang: Option<&str>,
    ) -> Vec<f32> {
        experts
            .iter()
            .enumerate()
            .map(|(i, expert)| {
                let fit = expert.schema_fit(symbol_path); // 0..1
                let fit_cost = (1.0 - fit) * self.fit_weight;
                let profile_cost = profile_prior.get(i).copied().unwrap_or(0.0) * self.profile_weight;
                let switch_cost = match context_lang {
                    Some(lang) if lang == expert.lang() => 0.0,
                    Some(_) => self.context_weight,
                    None => 0.0,
                };
                fit_cost + profile_cost + switch_cost
            })
            .collect()
    }
}

/// A coordinated, language-tagged candidate.
#[derive(Clone, Debug, PartialEq)]
pub struct ScoredCandidate {
    pub lang: String,
    pub candidate: ReadingCandidate,
    pub score: f32,
}

/// Coordinator: gate → run activated experts → merge with the gate cost → rank.
/// Sparse activation: experts whose gate is far worse than the best are skipped.
pub struct Coordinator {
    pub router: Router,
    /// Skip experts whose gate exceeds `best_gate + activation_margin`.
    pub activation_margin: f32,
    /// How strongly the gate cost feeds into the final candidate score.
    pub gate_weight: f32,
}

impl Default for Coordinator {
    fn default() -> Self {
        Self {
            router: Router::default(),
            activation_margin: 10.0,
            gate_weight: 1.0,
        }
    }
}

impl Coordinator {
    pub fn decode(
        &self,
        experts: &[&dyn LanguageExpert],
        symbol_path: &str,
        profile_prior: &[f32],
        context_lang: Option<&str>,
        limit: usize,
    ) -> Vec<ScoredCandidate> {
        let gates = self.router.gate(experts, symbol_path, profile_prior, context_lang);
        let best_gate = gates.iter().copied().fold(f32::INFINITY, f32::min);

        let mut out = Vec::new();
        for (i, expert) in experts.iter().enumerate() {
            let gate = gates[i];
            if gate > best_gate + self.activation_margin {
                continue; // sparse activation — this language is implausible here
            }
            for candidate in expert.candidates(symbol_path) {
                let score = candidate.cost + gate * self.gate_weight;
                out.push(ScoredCandidate {
                    lang: expert.lang().to_owned(),
                    candidate,
                    score,
                });
            }
        }
        out.sort_by(|a, b| a.score.total_cmp(&b.score));
        out.dedup_by(|a, b| a.candidate.text == b.candidate.text && a.lang == b.lang);
        out.truncate(limit);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use schema_en_word::EnglishWordSchema;
    use schema_zh_hans_pinyin_full::ZhHansPinyinFullSchema;

    #[test]
    fn router_fit_separates_languages() {
        let zh = ZhHansPinyinFullSchema::builtin();
        let en = EnglishWordSchema::builtin();
        let zh_expert = SchemaExpert::new("zh", &zh);
        let en_expert = SchemaExpert::new("en", &en);

        // "nihao" parses as pinyin, not as an English word.
        assert!(zh_expert.schema_fit("nihao") > en_expert.schema_fit("nihao"));
        // "keyboard" is an English word, not valid pinyin.
        assert!(en_expert.schema_fit("keyboard") > zh_expert.schema_fit("keyboard"));
    }

    #[test]
    fn coordinator_routes_to_correct_language() {
        let zh = ZhHansPinyinFullSchema::builtin();
        let en = EnglishWordSchema::builtin();
        let zh_expert = SchemaExpert::new("zh", &zh);
        let en_expert = SchemaExpert::new("en", &en);
        let experts: Vec<&dyn LanguageExpert> = vec![&zh_expert, &en_expert];
        let coord = Coordinator::default();
        let neutral = [0.0, 0.0];

        let zh_top = coord.decode(&experts, "nihao", &neutral, None, 5);
        assert_eq!(zh_top.first().map(|c| c.lang.as_str()), Some("zh"));
        assert_eq!(zh_top.first().map(|c| c.candidate.text.as_str()), Some("你好"));

        let en_top = coord.decode(&experts, "keyboard", &neutral, None, 5);
        assert_eq!(en_top.first().map(|c| c.lang.as_str()), Some("en"));
        assert_eq!(en_top.first().map(|c| c.candidate.text.as_str()), Some("keyboard"));
    }
}
