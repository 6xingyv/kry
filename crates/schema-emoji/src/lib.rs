use std::collections::HashMap;

use schema_core::{Boundary, CandidateSource, ReadingCandidate, Schema, SchemaId};

#[derive(Clone, Debug)]
pub struct EmojiSchema {
    id: SchemaId,
    aliases: HashMap<String, Vec<String>>,
}

impl EmojiSchema {
    pub fn new(id: impl Into<String>, aliases: HashMap<String, Vec<String>>) -> Self {
        Self {
            id: SchemaId::new(id),
            aliases,
        }
    }

    pub fn builtin_en() -> Self {
        Self {
            id: SchemaId::new("emoji-en"),
            aliases: HashMap::from([
                ("haha".to_owned(), vec!["😂".to_owned()]),
                ("laugh".to_owned(), vec!["😂".to_owned()]),
                ("birthday".to_owned(), vec!["🎂".to_owned()]),
            ]),
        }
    }

    pub fn builtin_zh_hans() -> Self {
        Self {
            id: SchemaId::new("emoji-zh-hans"),
            aliases: HashMap::from([
                ("哈哈".to_owned(), vec!["😂".to_owned()]),
                ("生日".to_owned(), vec!["🎂".to_owned()]),
                ("版权".to_owned(), vec!["©".to_owned()]),
            ]),
        }
    }
}

impl Default for EmojiSchema {
    fn default() -> Self {
        Self::builtin_en()
    }
}

impl Schema for EmojiSchema {
    fn id(&self) -> SchemaId {
        self.id.clone()
    }

    fn candidates(&self, symbol_path: &str) -> Vec<ReadingCandidate> {
        let reading = symbol_path.to_ascii_lowercase();
        self.aliases
            .get(&reading)
            .into_iter()
            .flatten()
            .map(|text| ReadingCandidate {
                reading: reading.clone(),
                boundary: Boundary::from_reading(&reading),
                text: text.clone(),
                cost: 2.0,
                source: CandidateSource::Exact,
            })
            .collect()
    }
}
