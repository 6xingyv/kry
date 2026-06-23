use data_core::{Lexicon, LexiconEntry};
use schema_core::{Boundary, CandidateSource, ReadingCandidate, Schema, SchemaId};

#[derive(Clone, Debug)]
pub struct RuCyrillicSchema {
    lexicon: Lexicon,
}

impl RuCyrillicSchema {
    pub fn new(lexicon: Lexicon) -> Self {
        Self { lexicon }
    }

    pub fn builtin() -> Self {
        Self::new(Lexicon::new([
            LexiconEntry::new("привет", "привет", 100.0),
            LexiconEntry::new("школа", "школа", 80.0),
            LexiconEntry::new("москва", "москва", 70.0),
        ]))
    }
}

impl Default for RuCyrillicSchema {
    fn default() -> Self {
        Self::builtin()
    }
}

impl Schema for RuCyrillicSchema {
    fn id(&self) -> SchemaId {
        SchemaId::new("ru-cyrillic")
    }

    fn candidates(&self, symbol_path: &str) -> Vec<ReadingCandidate> {
        let reading = symbol_path.to_lowercase();
        let mut candidates = self
            .lexicon
            .lookup_reading(&reading)
            .iter()
            .map(|entry| ReadingCandidate {
                reading: reading.clone(),
                boundary: Boundary::from_reading(&reading),
                text: entry.text.clone(),
                cost: 1.0 / (1.0 + entry.weight),
                source: CandidateSource::Exact,
            })
            .collect::<Vec<_>>();
        if candidates.is_empty() && reading.chars().all(is_cyrillic_letter) {
            candidates.push(ReadingCandidate {
                reading: reading.clone(),
                boundary: Boundary::from_reading(&reading),
                text: reading,
                cost: 6.0,
                source: CandidateSource::Raw,
            });
        }
        candidates
    }
}

fn is_cyrillic_letter(ch: char) -> bool {
    matches!(ch, 'а'..='я' | 'ё')
}
