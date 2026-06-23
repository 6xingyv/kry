mod constructors;
mod gesture;
pub mod moe;
mod observation_model;
mod session;
mod stream;

use context_core::{CommittedText, ContextCompiler, ContextPredictiveCache, TextContext};
use data_core::{LanguagePack, Lexicon, LexiconEntry};
use decoder_beam::BeamDecoder;
use decoder_core::{Candidate, CandidateList, ScoreBreakdown};
use geometry_core::{GeometryLayout, Point, SlotLattice};
use keymap_core::{KeyLayer, KeyMap};
use lm_core::LanguageModel;
use personal_core::PersonalPredictiveCache;
use profile_core::KeyboardProfile;
use schema_core::{CandidateSource, Schema};
use std::collections::HashMap;
use std::io;
use std::path::Path;

pub use gesture::{GestureTemplateMatch, debug_path_costs};
use gesture::{
    GestureTemplateStartIndex, RuntimeGestureTemplate, gesture_templates_from_pack,
    schema_guided_gesture_decode, score_gesture_templates_against,
};
use observation_model::{observation_distance_unit_from_pack, observation_slot_units_from_pack};

const TAP_OBSERVATION_COST_SCALE: f32 = 4.0;

pub struct ImeEngine {
    geometry: Box<dyn GeometryLayout>,
    keymaps: Vec<Box<dyn KeyMap>>,
    schemas: Vec<Box<dyn Schema>>,
    profile: KeyboardProfile,
    context_compiler: ContextCompiler,
    context: TextContext,
    personal: PersonalPredictiveCache,
    decoder: BeamDecoder,
    lm: Box<dyn LanguageModel>,
    observation_distance_unit: f32,
    observation_slot_units: HashMap<geometry_core::SlotId, f32>,
    gesture_templates: Vec<RuntimeGestureTemplate>,
    gesture_start_index: GestureTemplateStartIndex,
    pending_stream_points: Vec<Point>,
    pending_gesture_points: Vec<Point>,
    pending_stream_pause_positions: Vec<usize>,
    pending_stream_last_slot: Option<geometry_core::SlotId>,
    pending_stream_candidates: CandidateList,
    pending_gesture_scored_stream_len: usize,
    swipe_lm_session: lm_core::LmSession,
    swipe_accepted_text: String,
    /// MoE per-language expert LMs (language tag -> LM), e.g. "zh" -> assets/lm,
    /// "en" -> assets/lm-en. Each expert reranks only its own language's
    /// candidates with its own tokenizer. Empty = fall back to `lm`.
    expert_lms: HashMap<String, Box<dyn LanguageModel>>,
}

/// How many top geometric candidates the swipe LM re-ranks. Bounds LM forwards per
/// decode to keep the swipe interactive (avoid ANR) — see `rerank_with_swipe_lm`.
const SWIPE_LM_RERANK_TOPK: usize = 12;

/// Weight on the length-normalized swipe LM context bonus. Tuned as a tie-breaker:
/// large enough to flip a 1–2 point geometric/frequency gap when the LM is
/// confident, small enough not to override geometry when the LM is unsure.
/// Overridable via `KRY_LM_SCALE` for tuning sweeps.
fn swipe_lm_bonus_scale() -> f32 {
    static V: std::sync::OnceLock<f32> = std::sync::OnceLock::new();
    // 0.4 from the CUDA continuation sweep: with a small Wikipedia-trained model the LM works
    // best as a gentle tie-breaker. Higher weights let its still-noisy judgments
    // override good geometry and net accuracy drops.
    *V.get_or_init(|| env_f32("KRY_LM_SCALE", 0.4))
}

/// GNMT-style length penalty exponent for the LM bonus. 0 = raw sum (favors short
/// candidates), 1 = full per-character average (favors long). ~0.7 balances them.
/// Overridable via `KRY_LM_ALPHA`.
fn swipe_lm_length_penalty() -> f32 {
    static V: std::sync::OnceLock<f32> = std::sync::OnceLock::new();
    // 0.5 from the sweep (mild length penalty); α=1.0 over-favored long candidates.
    *V.get_or_init(|| env_f32("KRY_LM_ALPHA", 0.5))
}

fn env_f32(key: &str, default: f32) -> f32 {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

impl ImeEngine {
    pub fn set_lm(&mut self, lm: Box<dyn LanguageModel>) {
        self.lm = lm;
    }

    /// Loads a trained MiniGPT character LM from `root` (expects `config.json`,
    /// `tokenizer.json`, `model.safetensors`) and installs it. Without this the
    /// engine keeps the zero-cost `NullLanguageModel`, so the call is optional and
    /// non-breaking for callers that don't ship an LM.
    pub fn load_lm_from_dir(&mut self, root: impl AsRef<Path>) -> io::Result<()> {
        let root = root.as_ref();
        let model = lm_core::MiniGptModel::load_from_dir(root)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
        self.set_lm(Box::new(model));
        // MoE: auto-load sibling per-language expert LMs (assets/lm -> assets/lm-en).
        // The primary `lm` serves the profile's main language; experts handle the rest.
        if let (Some(parent), Some(name)) =
            (root.parent(), root.file_name().and_then(|n| n.to_str()))
        {
            for (lang, suffix) in [("en", "-en"), ("zh", "-zh")] {
                let dir = parent.join(format!("{name}{suffix}"));
                if dir.join("model.safetensors").exists() {
                    let _ = self.load_expert_lm(lang, &dir);
                }
            }
        }
        Ok(())
    }

    /// Load a per-language expert LM (MoE). `lang` is a tag like "zh"/"en"; `root`
    /// holds config.json/tokenizer.json/model.safetensors. Optional, non-breaking.
    pub fn load_expert_lm(
        &mut self,
        lang: impl Into<String>,
        root: impl AsRef<Path>,
    ) -> io::Result<()> {
        let model = lm_core::MiniGptModel::load_from_dir(root.as_ref())
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
        self.expert_lms.insert(lang.into(), Box::new(model));
        Ok(())
    }

    pub fn set_committed_context(&mut self, text: impl Into<String>) {
        self.context.committed = CommittedText { text: text.into() };
    }

    pub fn decode_taps(&self, points: &[Point]) -> CandidateList {
        let lattice = self.tap_lattice_for_points(points, 5);
        self.decode_lattice(&lattice)
    }

    pub fn decode_taps_composed(&self, points: &[Point]) -> decoder_core::CompositionResult {
        if self.profile.sentence_beam_budget == 0 {
            return decoder_core::CompositionResult::default();
        }

        let lattice = self.tap_lattice_for_points(points, 5);

        let context = self.context_compiler.compile_view(&self.context);
        let keymaps: Vec<&dyn KeyMap> = self.keymaps.iter().map(|k| k.as_ref()).collect();

        let incr_owned: Vec<_> = self
            .schemas
            .iter()
            .filter_map(|s| s.as_incremental().map(|incr| (s.id(), incr)))
            .collect();
        let incr_schemas: Vec<(&schema_core::SchemaId, &dyn schema_core::IncrementalSchema)> =
            incr_owned.iter().map(|(id, s)| (id, *s)).collect();

        if incr_schemas.is_empty() {
            return decoder_core::CompositionResult::default();
        }

        let mut result = self.decoder.decode_sentence(
            &lattice,
            &self.profile,
            &keymaps,
            &incr_schemas,
            self.lm.as_ref(),
            &context,
            &self.personal,
        );
        // A sentence with no CJK characters is a raw parse-failure (e.g. "zhesh",
        // "dx" composed as literal letters). Drop those so callers fall back to
        // single-tap decode, which surfaces 简拼/补全 candidates (dx→东西,
        // zhesh→这是). Genuine multi-word sentences (你好, 我们) keep their CJK.
        result
            .sentences
            .retain(|s| s.total_text.chars().any(|c| !c.is_ascii()));
        result
    }

    pub fn decode_lattice(&self, lattice: &SlotLattice) -> CandidateList {
        self.decode_lattice_with_boundary_hints(lattice, &[])
    }

    pub fn decode_lattice_with_boundary_hints(
        &self,
        lattice: &SlotLattice,
        boundary_hints: &[usize],
    ) -> CandidateList {
        let context = self.context_compiler.compile_view(&self.context);
        let keymaps = self
            .keymaps
            .iter()
            .map(|keymap| keymap.as_ref())
            .collect::<Vec<_>>();
        let schemas = self
            .schemas
            .iter()
            .map(|schema| schema.as_ref())
            .collect::<Vec<_>>();
        let mut list = self.decoder.decode_with_boundary_hints(
            lattice,
            &self.profile,
            &keymaps,
            &schemas,
            &context,
            &self.personal,
            boundary_hints,
        );
        self.apply_router_gate(&mut list);
        list
    }

    /// MoE router gate for tap: penalize candidates whose language fits the input
    /// poorly. Input-level (computed once per language from the top candidate's
    /// symbol path), so — like the gesture path — it routes a clear English word
    /// out of a Chinese profile without per-template flooding.
    fn apply_router_gate(&self, list: &mut CandidateList) {
        let Some(input_path) = list.candidates.first().map(|c| c.symbol_path.clone()) else {
            return;
        };
        let mut lang_fit: HashMap<&str, f32> = HashMap::new();
        for schema in &self.schemas {
            let cands = schema.candidates(&input_path);
            let total = cands.len();
            let fit = if total > 0 {
                cands
                    .iter()
                    .filter(|c| c.source != CandidateSource::Raw)
                    .count() as f32
                    / total as f32
            } else {
                0.0
            };
            lang_fit
                .entry(lang_for_schema(&schema.id().0))
                .and_modify(|m| *m = m.max(fit))
                .or_insert(fit);
        }
        for cand in &mut list.candidates {
            let fit = lang_fit
                .get(lang_for_schema(&cand.schema_id.0))
                .copied()
                .unwrap_or(1.0);
            cand.breakdown.switching += ROUTER_FIT_WEIGHT * (1.0 - fit);
            cand.score = cand.breakdown.total();
        }
        list.candidates.sort_by(|a, b| a.score.total_cmp(&b.score));
    }

    pub fn score_gesture_templates(
        &self,
        points: &[Point],
        limit: usize,
    ) -> Vec<GestureTemplateMatch> {
        score_gesture_templates_against(
            self.gesture_templates_for_start(points),
            points,
            limit,
            self.observation_distance_unit,
        )
    }

    pub fn decode_gesture_trace(&self, points: &[Point], limit: usize) -> CandidateList {
        let match_budget = (limit * 8).max(64);
        // Schema-guided DFS only. The FUTO gesture templates are English-only and use
        // coordinates that don't match Phone10ColGeometry, so they (a) never apply to
        // Chinese and (b) returned high-cost garbage that, being non-empty, BLOCKED
        // the DFS — e.g. "women"→wakefield, "the"→theaters. DFS handles both languages.
        let mut matches = Vec::new();
        for schema in &self.schemas {
            if let Some(incr) = schema.as_incremental() {
                for keymap in &self.keymaps {
                    matches.extend(schema_guided_gesture_decode(
                        points,
                        self.geometry.as_ref(),
                        keymap.as_ref(),
                        incr,
                        match_budget,
                        self.observation_distance_unit,
                    ));
                }
            }
        }
        matches.sort_by(|a, b| a.cost.total_cmp(&b.cost));
        matches.dedup_by(|a, b| a.template == b.template);
        matches.truncate(match_budget);
        self.decode_gesture_matches(&matches)
    }

    /// Debug: unscaled `log P(text | accepted_session)` — the raw quantity the
    /// swipe LM bonus is built from. Lets benchmarks see the context gap between a
    /// correct word and a competing one.
    pub fn swipe_lm_logprob(&self, text: &str) -> f32 {
        if self.swipe_lm_session.is_empty() {
            return 0.0;
        }
        let tokens = self.lm.encode(text);
        if tokens.is_empty() {
            return 0.0;
        }
        let history = self.swipe_lm_session.history();
        let baseline = self.lm.score_sequence(history);
        let mut full = history.to_vec();
        full.extend_from_slice(&tokens);
        self.lm.score_sequence(&full) - baseline
    }

    // Per-candidate bonus: kept for the no-session invariant test. Production decode
    // uses `rerank_with_swipe_lm` (top-K, shared baseline) to avoid per-candidate forwards.
    #[cfg(test)]
    fn swipe_lm_bonus(&self, candidate_text: &str) -> f32 {
        if self.swipe_lm_session.is_empty() {
            return 0.0;
        }
        let candidate_tokens = self.lm.encode(candidate_text);
        if candidate_tokens.is_empty() {
            return 0.0;
        }
        let history = self.swipe_lm_session.history();
        let baseline = self.lm.score_sequence(history);
        let mut full_seq = Vec::with_capacity(history.len() + candidate_tokens.len());
        full_seq.extend_from_slice(history);
        full_seq.extend_from_slice(&candidate_tokens);
        let extended = self.lm.score_sequence(&full_seq);
        // `score_sequence` sums per-character log-probs, so a raw sum favors SHORTER
        // candidates (one log-prob beats two) — biasing a swipe toward the fewest-
        // character reading (答案→但). Dividing fully by length over-corrects the
        // other way (a 3-char word wins on per-char average, 面临→面控制). A partial
        // `len^α` penalty (α≈0.7, GNMT-style) balances the two.
        let penalty = (candidate_tokens.len() as f32).powf(swipe_lm_length_penalty());
        (extended - baseline) / penalty * swipe_lm_bonus_scale()
    }

    fn decode_gesture_matches(&self, matches: &[GestureTemplateMatch]) -> CandidateList {
        if matches.is_empty() {
            return CandidateList::default();
        }

        let context = self.context_compiler.compile(&self.context);
        let keymaps_by_id = self
            .keymaps
            .iter()
            .map(|keymap| (keymap.id(), keymap.as_ref()))
            .collect::<HashMap<_, _>>();
        let schemas_by_id = self
            .schemas
            .iter()
            .map(|schema| (schema.id(), schema.as_ref()))
            .collect::<HashMap<_, _>>();
        let mut candidates = Vec::new();
        // Router (moe): best schema-fit per language across all templates. The whole
        // input either looks like a language or not — this is input-level, not
        // per-template, so it can't flood suppressed-language junk.
        let mut lang_fit: HashMap<&str, f32> = HashMap::new();

        for matched in matches {
            for keymap_activation in &self.profile.keymaps {
                let Some(keymap) = keymaps_by_id.get(&keymap_activation.keymap_id) else {
                    continue;
                };
                let Some(slot_path) = self.slot_path_for_symbol_path(*keymap, &matched.template)
                else {
                    continue;
                };
                for schema_activation in &self.profile.schemas {
                    let Some(schema) = schemas_by_id.get(&schema_activation.schema_id) else {
                        continue;
                    };
                    let schema_candidates = schema.candidates(&matched.template);
                    // Track this language's best fit (fraction of real, non-raw parses).
                    let fit_total = schema_candidates.len();
                    if fit_total > 0 {
                        let fit = schema_candidates
                            .iter()
                            .filter(|c| c.source != CandidateSource::Raw)
                            .count() as f32
                            / fit_total as f32;
                        let lang = lang_for_schema(&schema_activation.schema_id.0);
                        lang_fit
                            .entry(lang)
                            .and_modify(|m| *m = m.max(fit))
                            .or_insert(fit);
                    }
                    for reading_candidate in schema_candidates {
                        // A swipe gesture always traces a complete word, so the raw
                        // pinyin passthrough (text == reading) must never surface here —
                        // it is a tap-only fallback for committing literal syllables.
                        if reading_candidate.source == CandidateSource::Raw {
                            continue;
                        }
                        // LM context bonus is NOT applied here: a full LM forward
                        // per candidate (×100s of candidates) is what stalls the UI
                        // / triggers ANR. Geometry+predictive context score first;
                        // the LM re-ranks only the top-K survivors below.
                        let context_bonus = context.score_bonus_for_schema(
                            &schema_activation.schema_id.0,
                            &reading_candidate.reading,
                            &reading_candidate.text,
                        );
                        let personal_bonus = self
                            .personal
                            .score_bonus(&reading_candidate.reading, &reading_candidate.text);
                        let breakdown = ScoreBreakdown {
                            observation: gesture_observation_cost(matched.cost),
                            keymap: keymap_activation.prior_cost,
                            schema: gesture_schema_cost(
                                reading_candidate.cost,
                                &reading_candidate.text,
                                &matched.template,
                            ),
                            profile: schema_activation.prior_cost,
                            context: context_bonus,
                            personal: personal_bonus,
                            switching: 0.0,
                        };
                        candidates.push(Candidate {
                            text: reading_candidate.text,
                            reading: reading_candidate.reading,
                            boundary: reading_candidate.boundary,
                            slot_path: slot_path.clone(),
                            keymap_id: keymap_activation.keymap_id.clone(),
                            schema_id: schema_activation.schema_id.clone(),
                            symbol_path: matched.template.clone(),
                            score: breakdown.total(),
                            breakdown,
                        });
                    }
                }
            }
        }

        // Router gate (moe): penalize candidates whose language fits the whole input
        // poorly — applied per language (max fit), so it routes a clear English word
        // out of a Chinese trace without flooding like a per-template gate would.
        for cand in &mut candidates {
            let fit = lang_fit
                .get(lang_for_schema(&cand.schema_id.0))
                .copied()
                .unwrap_or(1.0);
            cand.breakdown.switching += ROUTER_FIT_WEIGHT * (1.0 - fit);
            cand.score = cand.breakdown.total();
        }

        let mut list = merge_candidate_lists([CandidateList { candidates }]);
        self.rerank_with_swipe_lm(&mut list);
        list
    }

    /// MoE per-language context rerank: each top-K candidate is scored by ITS
    /// language's expert LM (zh→`lm`/assets/lm, en→`expert_lms["en"]`/assets/lm-en),
    /// with the accepted-text history re-tokenized by that LM's own tokenizer.
    ///
    /// Context-only by design: a char LM models character transitions, not word
    /// frequency, so it improves cross-word coherence but NOT isolated-word choice
    /// (verified: it doesn't prefer getting>herring). Hence the empty-history guard
    /// stays — isolated words are left to lexicon frequency. One forward per
    /// language (not per candidate) keeps it cheap.
    fn rerank_with_swipe_lm(&self, list: &mut CandidateList) {
        if self.swipe_accepted_text.is_empty() || list.candidates.is_empty() {
            return;
        }
        let k = SWIPE_LM_RERANK_TOPK.min(list.candidates.len());
        let penalty_alpha = swipe_lm_length_penalty();
        let scale = swipe_lm_bonus_scale();

        let mut by_lang: HashMap<&str, Vec<usize>> = HashMap::new();
        for (i, cand) in list.candidates.iter().take(k).enumerate() {
            by_lang
                .entry(lang_for_schema(&cand.schema_id.0))
                .or_default()
                .push(i);
        }

        let mut bonuses = vec![0.0f32; k];
        for (lang, idxs) in &by_lang {
            let lm: &dyn LanguageModel = self
                .expert_lms
                .get(*lang)
                .map(|boxed| boxed.as_ref())
                .unwrap_or_else(|| self.lm.as_ref());
            let history = lm.encode(&self.swipe_accepted_text);
            let token_lists: Vec<Vec<u32>> =
                idxs.iter().map(|&i| lm.encode(&list.candidates[i].text)).collect();
            let refs: Vec<&[u32]> = token_lists.iter().map(Vec::as_slice).collect();
            let logps = lm.score_continuations(&history, &refs); // log P(cand | history)
            for ((&i, tokens), logp) in idxs.iter().zip(&token_lists).zip(logps) {
                if !tokens.is_empty() {
                    bonuses[i] = logp / (tokens.len() as f32).powf(penalty_alpha) * scale;
                }
            }
        }

        for (i, cand) in list.candidates.iter_mut().take(k).enumerate() {
            cand.breakdown.context += bonuses[i];
            cand.score = cand.breakdown.total();
        }
        list.candidates
            .sort_by(|a, b| a.score.total_cmp(&b.score));
    }

    fn install_gesture_templates(&mut self, templates: Vec<RuntimeGestureTemplate>) {
        self.gesture_templates = templates;
        self.gesture_start_index = GestureTemplateStartIndex::build(
            &self.gesture_templates,
            self.observation_distance_unit,
        );
    }

    fn gesture_templates_for_start<'a>(
        &'a self,
        points: &[Point],
    ) -> Box<dyn Iterator<Item = &'a RuntimeGestureTemplate> + 'a> {
        let Some(start) = points.first().copied() else {
            return Box::new(std::iter::empty());
        };
        if !self.gesture_start_index.is_indexed() {
            return Box::new(self.gesture_templates.iter());
        }
        let indices = self.gesture_start_index.nearby_indices(start);
        if indices.is_empty() {
            return Box::new(std::iter::empty());
        }
        Box::new(
            indices
                .into_iter()
                .filter_map(|index| self.gesture_templates.get(index)),
        )
    }

    fn slot_path_for_symbol_path(
        &self,
        keymap: &dyn KeyMap,
        symbol_path: &str,
    ) -> Option<Vec<geometry_core::SlotId>> {
        symbol_path
            .chars()
            .map(|ch| {
                self.geometry.slots().iter().find_map(|slot| {
                    keymap
                        .symbol_for_slot(&slot.id, KeyLayer::Normal)
                        .and_then(|symbol| (symbol.0 == ch.to_string()).then(|| slot.id.clone()))
                })
            })
            .collect()
    }

    fn tap_lattice_for_points(&self, points: &[Point], alternatives: usize) -> SlotLattice {
        if alternatives == 0 {
            return SlotLattice::default();
        }
        SlotLattice::new(
            points
                .iter()
                .map(|point| {
                    let mut slots = self
                        .geometry
                        .slots()
                        .iter()
                        .map(|slot| {
                            let center = slot.center();
                            let half_w = (slot.bounds.width * 0.5).max(0.0001);
                            let half_h = (slot.bounds.height * 0.5).max(0.0001);
                            let dx = (point.x - center.x) / half_w;
                            let dy = (point.y - center.y) / half_h;
                            geometry_core::SlotObservation {
                                slot_id: slot.id.clone(),
                                cost: (dx * dx + dy * dy) * TAP_OBSERVATION_COST_SCALE,
                            }
                        })
                        .collect::<Vec<_>>();
                    slots.sort_by(|a, b| a.cost.total_cmp(&b.cost));
                    slots.truncate(alternatives);
                    slots
                })
                .collect(),
        )
    }
}

fn merge_candidate_lists(lists: impl IntoIterator<Item = CandidateList>) -> CandidateList {
    let mut merged: HashMap<(String, String, schema_core::SchemaId), Candidate> = HashMap::new();
    for candidate in lists
        .into_iter()
        .flat_map(|list| list.candidates.into_iter())
    {
        let key = (
            candidate.text.clone(),
            candidate.reading.clone(),
            candidate.schema_id.clone(),
        );
        match merged.get(&key) {
            Some(existing) if existing.score <= candidate.score => {}
            _ => {
                merged.insert(key, candidate);
            }
        }
    }
    let mut candidates = merged.into_values().collect::<Vec<_>>();
    candidates.sort_by(|a, b| a.score.total_cmp(&b.score));
    CandidateList { candidates }
}

/// Map a schema id to its MoE language tag (which expert LM reranks it).
fn lang_for_schema(schema_id: &str) -> &'static str {
    if schema_id.starts_with("zh") {
        "zh"
    } else if schema_id.starts_with("en") {
        "en"
    } else if schema_id.starts_with("es") {
        "es"
    } else if schema_id.starts_with("ru") {
        "ru"
    } else if schema_id.starts_with("emoji") {
        "emoji"
    } else {
        "zh"
    }
}

/// moe::Router gate weight: how hard to penalize a candidate whose language fits
/// the whole input poorly. Applied per-LANGUAGE (max fit over templates), never
/// per-template — a per-template gate floods suppressed-language junk.
const ROUTER_FIT_WEIGHT: f32 = 8.0;

fn gesture_observation_cost(cost: f32) -> f32 {
    cost * gesture_obs_scale()
}

/// Observation weight relative to schema/frequency. Higher = trust geometry more
/// (better for rare words); lower = trust frequency more (better for common
/// words, i.e. the frequency-weighted metric). Env-tunable for sweeps.
fn gesture_obs_scale() -> f32 {
    use std::sync::OnceLock;
    static SCALE: OnceLock<f32> = OnceLock::new();
    *SCALE.get_or_init(|| {
        // 5.0 (was 8.0): the obs-scale sweep showed trusting frequency a bit more
        // lifts the frequency-weighted score across all noise levels (heavy
        // 90.5→91.1%) without hurting clean accuracy — better matches real device
        // noise. Env-overridable for further sweeps.
        std::env::var("KRY_OBS_SCALE")
            .ok()
            .and_then(|v| v.parse().ok())
            .filter(|v: &f32| *v > 0.0)
            .unwrap_or(5.0)
    })
}

fn gesture_schema_cost(cost: f32, _text: &str, _template: &str) -> f32 {
    cost
}

fn read_optional_emoji_pack(root: &Path, schema: &str) -> io::Result<Option<LanguagePack>> {
    let pack_root = root.join(schema);
    if !pack_root.join("manifest.json").exists() {
        return Ok(None);
    }
    LanguagePack::load(pack_root).map(Some)
}

fn emoji_aliases_from_pack(pack: &LanguagePack) -> Option<HashMap<String, Vec<String>>> {
    pack.alias_table.as_ref().map(|table| {
        table
            .aliases
            .iter()
            .map(|entry| (entry.alias.clone(), entry.outputs.clone()))
            .collect()
    })
}

fn context_cache_from_language_packs(packs: &[&LanguagePack]) -> ContextPredictiveCache {
    let mut cache = ContextPredictiveCache::empty();
    for pack in packs {
        if pack.manifest.schema == "zh-hans-pinyin-full" {
            add_zh_lexicon_continuations(&mut cache, &pack.manifest.schema, &pack.lexicon, 200_000);
        }
        if let Some(model) = pack.context_model.as_ref() {
            add_context_model_continuations(&mut cache, pack, model);
        }
    }
    cache.finalize(); // dedup/sort/truncate once after bulk load (see ContextPredictiveCache::finalize)
    cache
}

fn add_context_model_continuations(
    cache: &mut ContextPredictiveCache,
    pack: &LanguagePack,
    model: &data_core::ContextModelArtifact,
) {
    for continuation in &model.continuations {
        cache.add_schema_contextual_prediction(
            &pack.manifest.schema,
            &continuation.suffix,
            &continuation.reading,
            &continuation.text,
            continuation_bonus(continuation.weight as f32),
        );
    }
}

fn add_zh_lexicon_continuations(
    cache: &mut ContextPredictiveCache,
    schema: &str,
    lexicon: &Lexicon,
    entry_limit: usize,
) {
    for entry in lexicon.iter_entries().take(entry_limit) {
        for continuation in zh_continuations_from_entry(&entry) {
            cache.add_schema_contextual_prediction(
                schema,
                continuation.suffix,
                continuation.reading,
                continuation.text,
                continuation_bonus(entry.weight),
            );
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ContextContinuation {
    suffix: String,
    reading: String,
    text: String,
}

fn zh_continuations_from_entry(entry: &LexiconEntry) -> Vec<ContextContinuation> {
    let chars = entry.text.chars().collect::<Vec<_>>();
    let syllables = entry.reading.split_whitespace().collect::<Vec<_>>();
    if chars.len() < 2 || chars.len() > 6 || chars.len() != syllables.len() {
        return Vec::new();
    }
    if chars.iter().any(|ch| !is_cjk_unified(*ch)) {
        return Vec::new();
    }

    let max_suffix_len = chars.len().saturating_sub(1).min(3);
    (1..=max_suffix_len)
        .map(|split| ContextContinuation {
            suffix: chars[..split].iter().collect(),
            reading: syllables[split..].join(" "),
            text: chars[split..].iter().collect(),
        })
        .collect()
}

fn is_cjk_unified(ch: char) -> bool {
    ('\u{4e00}'..='\u{9fff}').contains(&ch)
}

fn continuation_bonus(weight: f32) -> f32 {
    (2.0 + (weight.max(1.0).ln() / 2.0)).clamp(2.0, 6.0)
}

#[cfg(test)]
mod tests;
