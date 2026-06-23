use data_core::{Lexicon, LexiconEntry};
use schema_core::{Boundary, CandidateSource, ReadingCandidate, Schema, SchemaId};

#[derive(Clone, Debug)]
pub struct RuTranslitSchema {
    lexicon: Lexicon,
}

impl RuTranslitSchema {
    pub fn new(lexicon: Lexicon) -> Self {
        Self { lexicon }
    }

    pub fn builtin() -> Self {
        Self::new(Lexicon::new([
            LexiconEntry::new("привет", "privet", 100.0),
            LexiconEntry::new("школа", "shkola", 80.0),
            LexiconEntry::new("москва", "moskva", 70.0),
        ]))
    }
}

impl Default for RuTranslitSchema {
    fn default() -> Self {
        Self::builtin()
    }
}

impl Schema for RuTranslitSchema {
    fn id(&self) -> SchemaId {
        SchemaId::new("ru-translit")
    }

    fn candidates(&self, symbol_path: &str) -> Vec<ReadingCandidate> {
        let reading = symbol_path.to_ascii_lowercase();
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
        candidates.push(ReadingCandidate {
            reading: reading.clone(),
            boundary: Boundary::from_reading(&reading),
            text: transliterate(&reading),
            cost: 4.0,
            source: CandidateSource::Raw,
        });
        candidates
    }
}

fn transliterate(value: &str) -> String {
    let rules = [
        ("shch", "щ"),
        ("yo", "ё"),
        ("zh", "ж"),
        ("kh", "х"),
        ("ts", "ц"),
        ("ch", "ч"),
        ("sh", "ш"),
        ("yu", "ю"),
        ("ya", "я"),
        ("a", "а"),
        ("b", "б"),
        ("v", "в"),
        ("g", "г"),
        ("d", "д"),
        ("e", "е"),
        ("z", "з"),
        ("i", "и"),
        ("j", "й"),
        ("k", "к"),
        ("l", "л"),
        ("m", "м"),
        ("n", "н"),
        ("o", "о"),
        ("p", "п"),
        ("r", "р"),
        ("s", "с"),
        ("t", "т"),
        ("u", "у"),
        ("f", "ф"),
        ("h", "х"),
        ("c", "к"),
        ("y", "ы"),
    ];
    let mut out = String::new();
    let mut index = 0;
    while index < value.len() {
        if let Some((latin, cyrillic)) = rules
            .iter()
            .find(|(latin, _)| value[index..].starts_with(*latin))
        {
            out.push_str(cyrillic);
            index += latin.len();
        } else {
            index += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transliterates_longest_rules_first() {
        let schema = RuTranslitSchema::builtin();
        let candidates = schema.candidates("privet");
        assert!(
            candidates
                .iter()
                .any(|candidate| candidate.text == "привет")
        );
        assert_eq!(transliterate("shchuka"), "щука");
    }
}
