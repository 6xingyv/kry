use std::collections::HashMap;

use geometry_core::SlotId;
use keymap_core::{KeyLayer, KeyMap, KeyMapId, MappedSymbol, Symbol};

#[derive(Clone, Debug)]
pub struct LatinSpanishKeyMap {
    normal: HashMap<SlotId, Symbol>,
    long_press: HashMap<SlotId, Vec<Symbol>>,
}

impl LatinSpanishKeyMap {
    pub fn new() -> Self {
        let rows = ["qwertyuiop", "asdfghjklñ", "zxcvbnm"];
        let mut normal = HashMap::new();
        let mut long_press = HashMap::new();

        for (row, chars) in rows.iter().enumerate() {
            for (col, ch) in chars.chars().enumerate() {
                let id = SlotId::new(format!("r{row}c{col}"));
                normal.insert(id.clone(), Symbol::new(ch.to_string()));
                let alternatives = match ch {
                    'a' => vec!["á"],
                    'e' => vec!["é"],
                    'i' => vec!["í"],
                    'o' => vec!["ó"],
                    'u' => vec!["ú", "ü"],
                    'n' => vec!["ñ"],
                    _ => Vec::new(),
                };
                if !alternatives.is_empty() {
                    long_press.insert(
                        id,
                        alternatives
                            .into_iter()
                            .map(Symbol::new)
                            .collect::<Vec<_>>(),
                    );
                }
            }
        }

        Self { normal, long_press }
    }

    pub fn slot_for_symbol(&self, symbol: char) -> Option<SlotId> {
        let symbol = symbol.to_lowercase().to_string();
        self.normal
            .iter()
            .find_map(|(slot, mapped)| (mapped.0 == symbol).then(|| slot.clone()))
    }
}

impl Default for LatinSpanishKeyMap {
    fn default() -> Self {
        Self::new()
    }
}

impl KeyMap for LatinSpanishKeyMap {
    fn id(&self) -> KeyMapId {
        KeyMapId::new("latin-spanish")
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

    fn alternatives_for_slot(&self, slot: &SlotId, layer: KeyLayer) -> Vec<MappedSymbol> {
        if layer != KeyLayer::Normal {
            return self
                .symbol_for_slot(slot, layer)
                .map(|symbol| vec![MappedSymbol { symbol, cost: 0.0 }])
                .unwrap_or_default();
        }
        let mut out = self
            .symbol_for_slot(slot, layer)
            .map(|symbol| vec![MappedSymbol { symbol, cost: 0.0 }])
            .unwrap_or_default();
        if let Some(alternatives) = self.long_press.get(slot) {
            out.extend(
                alternatives
                    .iter()
                    .cloned()
                    .map(|symbol| MappedSymbol { symbol, cost: 1.0 }),
            );
        }
        out
    }
}
