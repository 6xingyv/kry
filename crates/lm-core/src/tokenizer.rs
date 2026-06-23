use std::collections::HashMap;

use serde::{Deserialize, Serialize};

pub const PAD_TOKEN: u32 = 0;
pub const UNK_TOKEN: u32 = 1;
pub const EOS_TOKEN: u32 = 2;
const SPECIAL_COUNT: u32 = 3;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CharacterTokenizer {
    char_to_id: HashMap<char, u32>,
    id_to_char: Vec<char>,
    vocab_size: usize,
}

impl CharacterTokenizer {
    pub fn from_vocab(chars: &[char]) -> Self {
        let mut char_to_id = HashMap::with_capacity(chars.len());
        let mut id_to_char = Vec::with_capacity(SPECIAL_COUNT as usize + chars.len());

        id_to_char.push('\0'); // PAD
        id_to_char.push('\u{FFFD}'); // UNK
        id_to_char.push('\n'); // EOS

        for (i, &ch) in chars.iter().enumerate() {
            let id = SPECIAL_COUNT + i as u32;
            char_to_id.insert(ch, id);
            id_to_char.push(ch);
        }

        let vocab_size = id_to_char.len();
        Self {
            char_to_id,
            id_to_char,
            vocab_size,
        }
    }

    pub fn builtin() -> Self {
        let mut chars = Vec::with_capacity(8192);

        for byte in 0x20u8..=0x7E {
            chars.push(byte as char);
        }

        for cp in 0x4E00u32..=0x9FFF {
            if let Some(ch) = char::from_u32(cp) {
                chars.push(ch);
            }
        }

        for cp in 0x3400u32..=0x4DBF {
            if let Some(ch) = char::from_u32(cp) {
                chars.push(ch);
            }
        }

        for cp in 0x3000u32..=0x303F {
            if let Some(ch) = char::from_u32(cp) {
                chars.push(ch);
            }
        }
        for cp in 0xFF00u32..=0xFF5E {
            if let Some(ch) = char::from_u32(cp) {
                chars.push(ch);
            }
        }

        Self::from_vocab(&chars)
    }

    pub fn encode(&self, text: &str) -> Vec<u32> {
        text.chars()
            .map(|ch| self.char_to_id.get(&ch).copied().unwrap_or(UNK_TOKEN))
            .collect()
    }

    pub fn decode(&self, token_ids: &[u32]) -> String {
        token_ids
            .iter()
            .filter_map(|&id| {
                if id == PAD_TOKEN || id == EOS_TOKEN {
                    return None;
                }
                self.id_to_char.get(id as usize).copied()
            })
            .collect()
    }

    pub fn vocab_size(&self) -> usize {
        self.vocab_size
    }

    pub fn token_id(&self, ch: char) -> u32 {
        self.char_to_id.get(&ch).copied().unwrap_or(UNK_TOKEN)
    }

    pub fn from_json_path(path: &std::path::Path) -> std::io::Result<Self> {
        let data = std::fs::read_to_string(path)?;
        let json: serde_json::Value = serde_json::from_str(&data)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        let char_to_id_obj = json
            .get("char_to_id")
            .and_then(|v| v.as_object())
            .ok_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::InvalidData, "missing char_to_id")
            })?;

        let mut entries: Vec<(char, u32)> = Vec::with_capacity(char_to_id_obj.len());
        for (key, val) in char_to_id_obj {
            let ch = key
                .chars()
                .next()
                .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidData, "empty key"))?;
            let id = val.as_u64().ok_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::InvalidData, "non-integer id")
            })? as u32;
            entries.push((ch, id));
        }
        entries.sort_by_key(|&(_, id)| id);

        let max_id = entries
            .last()
            .map(|&(_, id)| id)
            .unwrap_or(SPECIAL_COUNT - 1);
        let mut id_to_char = vec!['\0'; (max_id + 1) as usize];
        id_to_char[PAD_TOKEN as usize] = '\0';
        id_to_char[UNK_TOKEN as usize] = '\u{FFFD}';
        id_to_char[EOS_TOKEN as usize] = '\n';

        let mut char_map = HashMap::with_capacity(entries.len());
        for (ch, id) in entries {
            char_map.insert(ch, id);
            if (id as usize) < id_to_char.len() {
                id_to_char[id as usize] = ch;
            }
        }

        Ok(Self {
            char_to_id: char_map,
            id_to_char,
            vocab_size: (max_id + 1) as usize,
        })
    }
}
