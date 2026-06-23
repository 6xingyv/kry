use std::collections::HashMap;

use geometry_core::SlotId;
use keymap_core::{KeyLayer, KeyMap, KeyMapId, Symbol};

#[derive(Clone, Debug)]
pub struct LatinQwertyKeyMap {
    normal: HashMap<SlotId, Symbol>,
    shifted: HashMap<SlotId, Symbol>,
}

impl LatinQwertyKeyMap {
    pub fn new() -> Self {
        let rows = ["qwertyuiop", "asdfghjkl", "zxcvbnm"];
        let mut normal = HashMap::new();
        let mut shifted = HashMap::new();

        for (row, chars) in rows.iter().enumerate() {
            for (col, ch) in chars.chars().enumerate() {
                let id = SlotId::new(format!("r{row}c{col}"));
                normal.insert(id.clone(), Symbol::new(ch.to_string()));
                shifted.insert(id, Symbol::new(ch.to_ascii_uppercase().to_string()));
            }
        }

        Self { normal, shifted }
    }

    pub fn slot_for_symbol(&self, symbol: char) -> Option<SlotId> {
        self.normal
            .iter()
            .find_map(|(slot, mapped)| (mapped.0 == symbol.to_string()).then(|| slot.clone()))
    }
}

impl Default for LatinQwertyKeyMap {
    fn default() -> Self {
        Self::new()
    }
}

impl KeyMap for LatinQwertyKeyMap {
    fn id(&self) -> KeyMapId {
        KeyMapId::new("latin-qwerty")
    }

    fn symbol_for_slot(&self, slot: &SlotId, layer: KeyLayer) -> Option<Symbol> {
        match layer {
            KeyLayer::Normal => self.normal.get(slot).cloned(),
            KeyLayer::Shift => self.shifted.get(slot).cloned(),
            KeyLayer::Alt | KeyLayer::Symbol => None,
        }
    }
}
