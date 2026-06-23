mod boundary;
mod merge;
pub mod sentence;

use std::collections::HashMap;

use context_core::ContextScorer;
use decoder_core::{Candidate, CandidateList, CompositionResult, ScoreBreakdown};
use geometry_core::{SlotId, SlotLattice};
use keymap_core::{KeyLayer, KeyMap, KeyMapId};
use lm_core::LanguageModel;
use personal_core::PersonalPredictiveCache;
use profile_core::KeyboardProfile;
use schema_core::{IncrementalSchema, Schema, SchemaId};

use boundary::boundary_hint_cost;
use merge::merge_candidates;

#[derive(Clone, Debug)]
pub struct BeamDecoder {
    pub symbol_beam: usize,
}

impl Default for BeamDecoder {
    fn default() -> Self {
        Self { symbol_beam: 64 }
    }
}

#[derive(Clone, Debug)]
struct SymbolHypothesis {
    slot_path: Vec<SlotId>,
    keymap_id: KeyMapId,
    symbol_path: String,
    observation_cost: f32,
    keymap_cost: f32,
    profile_cost: f32,
}

impl BeamDecoder {
    pub fn decode(
        &self,
        lattice: &SlotLattice,
        profile: &KeyboardProfile,
        keymaps: &[&dyn KeyMap],
        schemas: &[&dyn Schema],
        context: &dyn ContextScorer,
        personal: &PersonalPredictiveCache,
    ) -> CandidateList {
        self.decode_with_boundary_hints(lattice, profile, keymaps, schemas, context, personal, &[])
    }

    pub fn decode_with_boundary_hints(
        &self,
        lattice: &SlotLattice,
        profile: &KeyboardProfile,
        keymaps: &[&dyn KeyMap],
        schemas: &[&dyn Schema],
        context: &dyn ContextScorer,
        personal: &PersonalPredictiveCache,
        boundary_hints: &[usize],
    ) -> CandidateList {
        if lattice.is_empty() {
            return CandidateList::default();
        }

        let keymaps_by_id = keymaps
            .iter()
            .map(|keymap| (keymap.id(), *keymap))
            .collect::<HashMap<_, _>>();
        let schemas_by_id = schemas
            .iter()
            .map(|schema| (schema.id(), *schema))
            .collect::<HashMap<_, _>>();

        let mut symbol_hypotheses = Vec::new();
        for activation in &profile.keymaps {
            let Some(keymap) = keymaps_by_id.get(&activation.keymap_id) else {
                continue;
            };
            let mut beam = vec![SymbolHypothesis {
                slot_path: Vec::new(),
                keymap_id: activation.keymap_id.clone(),
                symbol_path: String::new(),
                observation_cost: 0.0,
                keymap_cost: 0.0,
                profile_cost: activation.prior_cost,
            }];

            for position in &lattice.positions {
                let mut next = Vec::new();
                for hyp in &beam {
                    for slot in position {
                        for mapped in keymap.alternatives_for_slot(&slot.slot_id, KeyLayer::Normal)
                        {
                            let mut slot_path = hyp.slot_path.clone();
                            slot_path.push(slot.slot_id.clone());
                            next.push(SymbolHypothesis {
                                slot_path,
                                keymap_id: hyp.keymap_id.clone(),
                                symbol_path: format!("{}{}", hyp.symbol_path, mapped.symbol.0),
                                observation_cost: hyp.observation_cost + slot.cost,
                                keymap_cost: hyp.keymap_cost + mapped.cost,
                                profile_cost: hyp.profile_cost,
                            });
                        }
                    }
                }
                next.sort_by(|a, b| {
                    (a.observation_cost + a.keymap_cost + a.profile_cost)
                        .total_cmp(&(b.observation_cost + b.keymap_cost + b.profile_cost))
                });
                next.truncate(self.symbol_beam);
                beam = next;
            }
            symbol_hypotheses.extend(beam);
        }

        let mut candidates = Vec::new();
        for activation in &profile.schemas {
            let Some(schema) = schemas_by_id.get(&activation.schema_id) else {
                continue;
            };
            let mut schema_candidates = Vec::new();
            for hyp in &symbol_hypotheses {
                for reading_candidate in schema.candidates(&hyp.symbol_path) {
                    let context_bonus = context.score_bonus_for_schema(
                        &activation.schema_id.0,
                        &reading_candidate.reading,
                        &reading_candidate.text,
                    );
                    let personal_bonus =
                        personal.score_bonus(&reading_candidate.reading, &reading_candidate.text);
                    let boundary_cost =
                        boundary_hint_cost(&reading_candidate.boundary, boundary_hints);
                    let breakdown = ScoreBreakdown {
                        observation: hyp.observation_cost,
                        keymap: hyp.keymap_cost,
                        schema: reading_candidate.cost + boundary_cost,
                        profile: hyp.profile_cost + activation.prior_cost,
                        context: context_bonus,
                        personal: personal_bonus,
                        switching: 0.0,
                    };
                    schema_candidates.push(Candidate {
                        text: reading_candidate.text,
                        reading: reading_candidate.reading,
                        boundary: reading_candidate.boundary,
                        slot_path: hyp.slot_path.clone(),
                        keymap_id: hyp.keymap_id.clone(),
                        schema_id: activation.schema_id.clone(),
                        symbol_path: hyp.symbol_path.clone(),
                        score: breakdown.total(),
                        breakdown,
                    });
                }
            }
            schema_candidates.sort_by(|a, b| a.score.total_cmp(&b.score));
            schema_candidates.truncate(activation.beam_budget);
            candidates.extend(schema_candidates);
        }

        candidates = merge_candidates(candidates);
        candidates.sort_by(|a, b| a.score.total_cmp(&b.score));
        CandidateList { candidates }
    }

    pub fn decode_sentence(
        &self,
        lattice: &SlotLattice,
        profile: &KeyboardProfile,
        keymaps: &[&dyn KeyMap],
        schemas: &[(&SchemaId, &dyn IncrementalSchema)],
        lm: &dyn LanguageModel,
        context: &dyn ContextScorer,
        personal: &PersonalPredictiveCache,
    ) -> CompositionResult {
        let budget = profile.sentence_beam_budget;
        if budget == 0 {
            return CompositionResult::default();
        }
        sentence::decode_sentence(
            lattice, profile, keymaps, schemas, lm, context, personal, budget,
        )
    }
}
