use geometry_core::SlotId;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct KeyMapId(pub String);

impl KeyMapId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum KeyLayer {
    Normal,
    Shift,
    Alt,
    Symbol,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Symbol(pub String);

impl Symbol {
    pub fn new(symbol: impl Into<String>) -> Self {
        Self(symbol.into())
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct MappedSymbol {
    pub symbol: Symbol,
    pub cost: f32,
}

pub trait KeyMap {
    fn id(&self) -> KeyMapId;
    fn symbol_for_slot(&self, slot: &SlotId, layer: KeyLayer) -> Option<Symbol>;

    fn alternatives_for_slot(&self, slot: &SlotId, layer: KeyLayer) -> Vec<MappedSymbol> {
        self.symbol_for_slot(slot, layer)
            .map(|symbol| vec![MappedSymbol { symbol, cost: 0.0 }])
            .unwrap_or_default()
    }
}
