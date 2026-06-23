use geometry_core::SlotId;
use keymap_core::KeyMapId;
use schema_core::{Boundary, SchemaId};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ScoreBreakdown {
    pub observation: f32,
    pub keymap: f32,
    pub schema: f32,
    pub profile: f32,
    pub context: f32,
    pub personal: f32,
    pub switching: f32,
}

impl ScoreBreakdown {
    pub fn total(&self) -> f32 {
        self.observation + self.keymap + self.schema + self.profile + self.switching
            - self.context
            - self.personal
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Candidate {
    pub text: String,
    pub reading: String,
    pub boundary: Boundary,
    pub slot_path: Vec<SlotId>,
    pub keymap_id: KeyMapId,
    pub schema_id: SchemaId,
    pub symbol_path: String,
    pub score: f32,
    pub breakdown: ScoreBreakdown,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct CandidateList {
    pub candidates: Vec<Candidate>,
}

impl CandidateList {
    pub fn top(&self) -> Option<&Candidate> {
        self.candidates.first()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ComposedWord {
    pub text: String,
    pub reading: String,
    pub boundary: Boundary,
    pub symbol_range: (usize, usize),
    pub schema_id: SchemaId,
    pub score: ScoreBreakdown,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SentenceCandidate {
    pub words: Vec<ComposedWord>,
    pub total_text: String,
    pub total_score: f32,
    pub pending_symbols: String,
}

#[derive(Clone, Debug, Default)]
pub struct CompositionResult {
    pub sentences: Vec<SentenceCandidate>,
    pub word_candidates: CandidateList,
}
