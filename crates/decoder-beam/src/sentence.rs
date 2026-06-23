use context_core::ContextScorer;
use decoder_core::{ComposedWord, CompositionResult, ScoreBreakdown, SentenceCandidate};
use geometry_core::{SlotId, SlotLattice};
use keymap_core::{KeyLayer, KeyMap, KeyMapId};
use lm_core::LanguageModel;
use personal_core::PersonalPredictiveCache;
use profile_core::KeyboardProfile;
use schema_core::{IncrementalSchema, ReadingCandidate, SchemaId, SchemaStateId};

use std::collections::HashMap;

#[derive(Clone)]
struct SentenceHypothesis {
    completed_words: Vec<ComposedWord>,
    schema_state: SchemaStateId,
    current_symbol_start: usize,
    slot_path: Vec<SlotId>,
    keymap_id: KeyMapId,
    schema_id: SchemaId,
    symbol_path: String,
    observation_cost: f32,
    keymap_cost: f32,
    profile_cost: f32,
    lm_score: f32,
    committed_text: String,
    committed_token_ids: Vec<u32>,
}

/// Penalty per committed word. Without it the beam over-segments a single syllable
/// into several cheap single-char interjections (jian → 及啊嗯). Discourages adding
/// words unless the evidence/LM justifies the split.
const WORD_INSERTION_PENALTY: f32 = 3.0;

/// Weight on the sentence LM negative-log-likelihood. The LM term is a coherence
/// cost: incoherent sequences (你铪哦) have high NLL and must cost more, not less.
const LM_WEIGHT: f32 = 0.5;

/// Number of finalized sentences the LM re-ranks (one forward each). Bounds LM
/// forwards per decode so tap composition stays interactive on the UI thread.
const LM_RERANK_TOPK: usize = 8;

impl SentenceHypothesis {
    fn total_cost(&self) -> f32 {
        let word_cost: f32 = self.completed_words.iter().map(|w| w.score.total()).sum();
        let word_penalty = WORD_INSERTION_PENALTY * self.completed_words.len() as f32;
        // `lm_score` is the sentence NLL (= -score_sequence ≥ 0). It is ADDED as a
        // cost: a coherent sentence has low NLL and should rank lower. The previous
        // `- self.lm_score` had the sign inverted, which rewarded the *least* likely
        // sequences and produced garbage like 及啊嗯 / 你铪哦.
        self.observation_cost
            + self.keymap_cost
            + word_cost
            + self.profile_cost
            + self.lm_score * LM_WEIGHT
            + word_penalty
    }
}

pub(crate) fn decode_sentence(
    lattice: &SlotLattice,
    profile: &KeyboardProfile,
    keymaps: &[&dyn KeyMap],
    schemas: &[(&SchemaId, &dyn IncrementalSchema)],
    lm: &dyn LanguageModel,
    context: &dyn ContextScorer,
    personal: &PersonalPredictiveCache,
    beam_budget: usize,
) -> CompositionResult {
    if lattice.is_empty() || beam_budget == 0 || schemas.is_empty() {
        return CompositionResult::default();
    }

    let keymaps_by_id: HashMap<_, _> = keymaps.iter().map(|k| (k.id(), *k)).collect();
    let schemas_by_id: HashMap<_, _> = schemas
        .iter()
        .map(|(id, schema)| ((*id).clone(), *schema))
        .collect();

    let mut beam: Vec<SentenceHypothesis> = Vec::new();
    for keymap_activation in &profile.keymaps {
        let Some(_keymap) = keymaps_by_id.get(&keymap_activation.keymap_id) else {
            continue;
        };
        for schema_activation in &profile.schemas {
            let Some(schema) = schemas_by_id.get(&schema_activation.schema_id) else {
                continue;
            };
            if schema_activation.beam_budget == 0 {
                continue;
            }
            schema.reset_arena();
            beam.push(SentenceHypothesis {
                completed_words: Vec::new(),
                schema_state: schema.initial_state(),
                current_symbol_start: 0,
                slot_path: Vec::new(),
                keymap_id: keymap_activation.keymap_id.clone(),
                schema_id: schema_activation.schema_id.clone(),
                symbol_path: String::new(),
                observation_cost: 0.0,
                keymap_cost: 0.0,
                profile_cost: keymap_activation.prior_cost + schema_activation.prior_cost,
                lm_score: 0.0,
                committed_text: String::new(),
                committed_token_ids: Vec::new(),
            });
        }
    }

    for (pos_idx, position) in lattice.positions.iter().enumerate() {
        let mut next_beam: Vec<SentenceHypothesis> = Vec::new();

        for hyp in &beam {
            let Some(keymap) = keymaps_by_id.get(&hyp.keymap_id) else {
                continue;
            };
            let Some(schema) = schemas_by_id.get(&hyp.schema_id) else {
                continue;
            };

            for slot in position {
                for mapped in keymap.alternatives_for_slot(&slot.slot_id, KeyLayer::Normal) {
                    let symbol = mapped.symbol.0.chars().next().unwrap_or('\0');
                    let advance_results = schema.advance(&hyp.schema_state, symbol);

                    for adv in &advance_results {
                        let mut slot_path = hyp.slot_path.clone();
                        slot_path.push(slot.slot_id.clone());
                        let symbol_path = format!("{}{}", hyp.symbol_path, mapped.symbol.0);

                        if adv.next_state.alive {
                            next_beam.push(SentenceHypothesis {
                                completed_words: hyp.completed_words.clone(),
                                schema_state: adv.next_state.clone(),
                                current_symbol_start: hyp.current_symbol_start,
                                slot_path: slot_path.clone(),
                                keymap_id: hyp.keymap_id.clone(),
                                schema_id: hyp.schema_id.clone(),
                                symbol_path: symbol_path.clone(),
                                observation_cost: hyp.observation_cost + slot.cost,
                                keymap_cost: hyp.keymap_cost + mapped.cost,
                                profile_cost: hyp.profile_cost,
                                lm_score: hyp.lm_score,
                                committed_text: hyp.committed_text.clone(),
                                committed_token_ids: hyp.committed_token_ids.clone(),
                            });
                        }

                        for completed in &adv.completed {
                            next_beam.push(complete_hypothesis(
                                hyp,
                                completed,
                                schema.initial_state(),
                                pos_idx + 1,
                                slot_path.clone(),
                                symbol_path.clone(),
                                hyp.observation_cost + slot.cost,
                                hyp.keymap_cost + mapped.cost,
                                lm,
                                context,
                                personal,
                            ));
                        }
                    }
                }
            }
        }

        next_beam.sort_by(|a, b| a.total_cost().total_cmp(&b.total_cost()));
        next_beam.truncate(beam_budget);
        beam = next_beam;
    }

    let mut finalized = Vec::new();
    for hyp in beam {
        let pending_symbols = hyp
            .symbol_path
            .get(hyp.current_symbol_start..)
            .unwrap_or_default();

        // Only a hypothesis that has consumed the whole input is a valid sentence.
        // Previously partial commits (e.g. "ji"→及 leaving "an" pending) were
        // finalized as-is and, having skipped the cost of the unconsumed tail,
        // out-ranked the full decode (jian → 见).
        if !hyp.completed_words.is_empty() && pending_symbols.is_empty() {
            finalized.push(hyp.clone());
        }

        if pending_symbols.is_empty() {
            continue;
        }
        let Some(schema) = schemas_by_id.get(&hyp.schema_id) else {
            continue;
        };

        for candidate in schema.candidates_at(&hyp.schema_state) {
            if !reading_matches_symbol_path(&candidate.reading, pending_symbols) {
                continue;
            }
            finalized.push(complete_hypothesis(
                &hyp,
                &candidate,
                schema.initial_state(),
                lattice.positions.len(),
                hyp.slot_path.clone(),
                hyp.symbol_path.clone(),
                hyp.observation_cost,
                hyp.keymap_cost,
                lm,
                context,
                personal,
            ));
        }
    }

    // Deferred LM pass: rank finalized sentences by geometry+word cost, then run the
    // LM forward only on the top-K survivors (one forward each) and fold the sentence
    // NLL into their cost. Bounds LM forwards per decode instead of one-per-hypothesis.
    finalized.sort_by(|a, b| a.total_cost().total_cmp(&b.total_cost()));
    let lm_k = LM_RERANK_TOPK.min(finalized.len());
    for hyp in finalized.iter_mut().take(lm_k) {
        if hyp.committed_token_ids.len() >= 2 {
            hyp.lm_score = -lm.score_sequence(&hyp.committed_token_ids);
        }
    }

    let mut sentences: Vec<SentenceCandidate> = finalized
        .into_iter()
        .map(sentence_from_hypothesis)
        .collect();
    sentences.sort_by(|a, b| {
        a.total_score
            .total_cmp(&b.total_score)
            .then_with(|| a.total_text.cmp(&b.total_text))
    });
    sentences.dedup_by(|a, b| a.total_text == b.total_text);
    sentences.truncate(beam_budget);

    CompositionResult {
        sentences,
        ..Default::default()
    }
}

fn complete_hypothesis(
    hyp: &SentenceHypothesis,
    completed: &ReadingCandidate,
    next_state: SchemaStateId,
    symbol_end: usize,
    slot_path: Vec<SlotId>,
    symbol_path: String,
    observation_cost: f32,
    keymap_cost: f32,
    lm: &dyn LanguageModel,
    context: &dyn ContextScorer,
    personal: &PersonalPredictiveCache,
) -> SentenceHypothesis {
    let mut committed_text = hyp.committed_text.clone();
    committed_text.push_str(&completed.text);
    let mut committed_tokens = hyp.committed_token_ids.clone();
    committed_tokens.extend(lm.encode(&completed.text)); // encode is cheap (no forward)

    // LM scoring is DEFERRED: running score_sequence (a full forward) for every beam
    // hypothesis here ran dozens of forwards per keystroke on the UI thread → ANR.
    // The beam prunes on geometry + word cost; the LM re-ranks only the top-K final
    // sentences (see the deferred pass in decode_sentence).
    let lm_score = 0.0;

    let context_bonus =
        context.score_bonus_for_schema(&hyp.schema_id.0, &completed.reading, &completed.text);
    let personal_bonus = personal.score_bonus(&completed.reading, &completed.text);

    let word = ComposedWord {
        text: completed.text.clone(),
        reading: completed.reading.clone(),
        boundary: completed.boundary.clone(),
        symbol_range: (hyp.current_symbol_start, symbol_end),
        schema_id: hyp.schema_id.clone(),
        score: ScoreBreakdown {
            observation: 0.0,
            keymap: 0.0,
            schema: completed.cost,
            profile: 0.0,
            context: context_bonus,
            personal: personal_bonus,
            switching: 0.0,
        },
    };

    let mut words = hyp.completed_words.clone();
    words.push(word);

    SentenceHypothesis {
        completed_words: words,
        schema_state: next_state,
        current_symbol_start: symbol_end,
        slot_path,
        keymap_id: hyp.keymap_id.clone(),
        schema_id: hyp.schema_id.clone(),
        symbol_path,
        observation_cost,
        keymap_cost,
        profile_cost: hyp.profile_cost,
        lm_score,
        committed_text,
        committed_token_ids: committed_tokens,
    }
}

fn sentence_from_hypothesis(hyp: SentenceHypothesis) -> SentenceCandidate {
    let total_text = hyp
        .completed_words
        .iter()
        .map(|w| w.text.as_str())
        .collect::<String>();
    let pending = hyp
        .symbol_path
        .get(hyp.current_symbol_start..)
        .unwrap_or_default()
        .to_owned();
    SentenceCandidate {
        total_score: hyp.total_cost(),
        words: hyp.completed_words,
        total_text,
        pending_symbols: pending,
    }
}

fn reading_matches_symbol_path(reading: &str, symbols: &str) -> bool {
    let compact = reading.split_whitespace().collect::<String>();
    compact.eq_ignore_ascii_case(symbols)
}
