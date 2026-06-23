use data_core::{Lexicon, LexiconEntry};
use schema_core::{Boundary, CandidateSource, ReadingCandidate, Schema, SchemaId};

#[derive(Clone, Debug)]
pub struct SpanishWordSchema {
    lexicon: Lexicon,
}

impl SpanishWordSchema {
    pub fn new(lexicon: Lexicon) -> Self {
        Self { lexicon }
    }

    pub fn builtin() -> Self {
        Self::new(Lexicon::new([
            LexiconEntry::new("mañana", "manana", 100.0),
            LexiconEntry::new("café", "cafe", 90.0),
            LexiconEntry::new("está", "esta", 80.0),
            LexiconEntry::new("esta", "esta", 60.0),
        ]))
    }
}

impl Default for SpanishWordSchema {
    fn default() -> Self {
        Self::builtin()
    }
}

impl Schema for SpanishWordSchema {
    fn id(&self) -> SchemaId {
        SchemaId::new("es-word")
    }

    fn candidates(&self, symbol_path: &str) -> Vec<ReadingCandidate> {
        let normalized = fold_spanish(symbol_path);
        let mut candidates = self
            .lexicon
            .lookup_reading(&normalized)
            .iter()
            .map(|entry| ReadingCandidate {
                reading: normalized.clone(),
                boundary: Boundary::from_reading(&normalized),
                text: entry.text.clone(),
                cost: 1.0 / (1.0 + entry.weight),
                source: CandidateSource::Exact,
            })
            .collect::<Vec<_>>();

        candidates.push(ReadingCandidate {
            reading: normalized.clone(),
            boundary: Boundary::from_reading(&normalized),
            text: normalized,
            cost: 6.0,
            source: CandidateSource::Raw,
        });
        candidates
    }
}

fn fold_spanish(value: &str) -> String {
    value
        .chars()
        .filter_map(|ch| match ch.to_ascii_lowercase() {
            'á' => Some('a'),
            'é' => Some('e'),
            'í' => Some('i'),
            'ó' => Some('o'),
            'ú' | 'ü' => Some('u'),
            'ñ' => Some('n'),
            ch if ch.is_ascii_alphabetic() => Some(ch),
            _ => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restores_accents_from_plain_input() {
        let schema = SpanishWordSchema::builtin();
        let candidates = schema.candidates("manana");
        assert!(
            candidates
                .iter()
                .any(|candidate| candidate.text == "mañana")
        );
    }
}
