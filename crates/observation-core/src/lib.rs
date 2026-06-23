#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ObservationPoint {
    pub x: f32,
    pub y: f32,
    pub t_ms: u64,
}

impl ObservationPoint {
    pub fn new(x: f32, y: f32, t_ms: u64) -> Self {
        Self { x, y, t_ms }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum RawInputEvent {
    Tap(ObservationPoint),
    Trace(Vec<ObservationPoint>),
    HardwareKey { key: String, t_ms: u64 },
    Delete { count: usize },
    Commit { text: String },
    Pause { duration_ms: u64 },
    AcceptCandidate { text: String, reading: String },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputSource {
    Touch,
    Trace,
    Hardware,
    System,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ObservationChunk {
    pub source: InputSource,
    pub events: Vec<RawInputEvent>,
}

impl ObservationChunk {
    pub fn new(source: InputSource, events: Vec<RawInputEvent>) -> Self {
        Self { source, events }
    }
}
