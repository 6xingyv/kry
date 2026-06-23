use std::collections::HashMap;

use geometry_core::SlotId;
use keymap_core::{KeyLayer, KeyMap, KeyMapId, Symbol};

#[derive(Clone, Debug)]
pub struct RuJcukenKeyMap {
    normal: HashMap<SlotId, Symbol>,
}

impl RuJcukenKeyMap {
    pub fn new() -> Self {
        let rows = ["йцукенгшщз", "фывапролдж", "ячсмитьбю"];
        let mut normal = HashMap::new();
        for (row, chars) in rows.iter().enumerate() {
            for (col, ch) in chars.chars().enumerate() {
                normal.insert(
                    SlotId::new(format!("r{row}c{col}")),
                    Symbol::new(ch.to_string()),
                );
            }
        }
        Self { normal }
    }

    pub fn slot_for_symbol(&self, symbol: char) -> Option<SlotId> {
        let symbol = symbol.to_lowercase().to_string();
        self.normal
            .iter()
            .find_map(|(slot, mapped)| (mapped.0 == symbol).then(|| slot.clone()))
    }
}

impl Default for RuJcukenKeyMap {
    fn default() -> Self {
        Self::new()
    }
}

impl KeyMap for RuJcukenKeyMap {
    fn id(&self) -> KeyMapId {
        KeyMapId::new("ru-jcuken")
    }

    fn symbol_for_slot(&self, slot: &SlotId, layer: KeyLayer) -> Option<Symbol> {
        match layer {
            KeyLayer::Normal => self.normal.get(slot).cloned(),
            KeyLayer::Shift => self
                .normal
                .get(slot)
                .map(|symbol| Symbol::new(symbol.0.to_uppercase())),
            KeyLayer::Alt | KeyLayer::Symbol => None,
        }
    }
}
