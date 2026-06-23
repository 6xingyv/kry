use std::collections::HashMap;

use decoder_core::Candidate;
use schema_core::SchemaId;

pub(super) fn merge_candidates(candidates: Vec<Candidate>) -> Vec<Candidate> {
    let mut merged: HashMap<(String, String, SchemaId), Candidate> = HashMap::new();
    for candidate in candidates {
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
    merged.into_values().collect()
}
