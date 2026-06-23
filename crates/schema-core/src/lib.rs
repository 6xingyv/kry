#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct SchemaId(pub String);

impl SchemaId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Boundary {
    pub segments: Vec<String>,
}

impl Boundary {
    pub fn from_reading(reading: &str) -> Self {
        Self {
            segments: reading
                .split_whitespace()
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ReadingCandidate {
    pub reading: String,
    pub boundary: Boundary,
    pub text: String,
    pub cost: f32,
    pub source: CandidateSource,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CandidateSource {
    Exact,
    Fuzzy,
    Prefix,
    Raw,
}

pub trait Schema {
    fn id(&self) -> SchemaId;
    fn candidates(&self, symbol_path: &str) -> Vec<ReadingCandidate>;
    fn as_incremental(&self) -> Option<&dyn IncrementalSchema> {
        None
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SchemaStateId {
    pub state_index: u32,
    pub accumulated_cost: f32,
    pub symbol_count: u16,
    pub alive: bool,
}

#[derive(Clone, Debug)]
pub struct SchemaAdvanceResult {
    pub next_state: SchemaStateId,
    pub cost_delta: f32,
    pub completed: Vec<ReadingCandidate>,
}

pub trait IncrementalSchema: Schema {
    fn initial_state(&self) -> SchemaStateId;
    fn advance(&self, state: &SchemaStateId, symbol: char) -> Vec<SchemaAdvanceResult>;
    fn candidates_at(&self, state: &SchemaStateId) -> Vec<ReadingCandidate>;

    fn is_alive(&self, state: &SchemaStateId) -> bool {
        state.alive
    }

    fn reset_arena(&self);
}
