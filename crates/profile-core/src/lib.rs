use keymap_core::KeyMapId;
use schema_core::SchemaId;

#[derive(Clone, Debug, PartialEq)]
pub struct KeyMapActivation {
    pub keymap_id: KeyMapId,
    pub prior_cost: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SchemaActivation {
    pub schema_id: SchemaId,
    pub prior_cost: f32,
    pub beam_budget: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct KeyboardProfile {
    pub id: String,
    pub keymaps: Vec<KeyMapActivation>,
    pub schemas: Vec<SchemaActivation>,
    pub language_switch_cost: f32,
    pub keymap_switch_cost: f32,
    pub sentence_beam_budget: usize,
}

impl KeyboardProfile {
    pub fn zh_qwerty() -> Self {
        Self {
            id: "zh-qwerty".to_owned(),
            keymaps: vec![KeyMapActivation {
                keymap_id: KeyMapId::new("latin-qwerty"),
                prior_cost: 0.0,
            }],
            schemas: vec![
                SchemaActivation {
                    schema_id: SchemaId::new("zh-hans-pinyin-full"),
                    prior_cost: 0.0,
                    beam_budget: 32,
                },
                SchemaActivation {
                    schema_id: SchemaId::new("en-word"),
                    prior_cost: 15.0,
                    beam_budget: 4,
                },
                SchemaActivation {
                    schema_id: SchemaId::new("emoji-zh-hans"),
                    prior_cost: 6.0,
                    beam_budget: 2,
                },
            ],
            language_switch_cost: 1.5,
            keymap_switch_cost: 2.0,
            sentence_beam_budget: 16,
        }
    }

    pub fn en_qwerty() -> Self {
        Self {
            id: "en-qwerty".to_owned(),
            keymaps: vec![KeyMapActivation {
                keymap_id: KeyMapId::new("latin-qwerty"),
                prior_cost: 0.0,
            }],
            schemas: vec![
                SchemaActivation {
                    schema_id: SchemaId::new("en-word"),
                    prior_cost: 0.0,
                    beam_budget: 28,
                },
                SchemaActivation {
                    schema_id: SchemaId::new("emoji-en"),
                    prior_cost: 6.0,
                    beam_budget: 2,
                },
            ],
            language_switch_cost: 1.5,
            keymap_switch_cost: 2.0,
            sentence_beam_budget: 0,
        }
    }

    pub fn en_es_qwerty() -> Self {
        Self {
            id: "en-es-qwerty".to_owned(),
            keymaps: vec![KeyMapActivation {
                keymap_id: KeyMapId::new("latin-spanish"),
                prior_cost: 0.0,
            }],
            schemas: vec![
                SchemaActivation {
                    schema_id: SchemaId::new("en-word"),
                    prior_cost: 0.0,
                    beam_budget: 28,
                },
                SchemaActivation {
                    schema_id: SchemaId::new("es-word"),
                    prior_cost: 2.0,
                    beam_budget: 8,
                },
                SchemaActivation {
                    schema_id: SchemaId::new("emoji-en"),
                    prior_cost: 6.0,
                    beam_budget: 2,
                },
            ],
            language_switch_cost: 1.5,
            keymap_switch_cost: 2.0,
            sentence_beam_budget: 0,
        }
    }

    pub fn ru_native() -> Self {
        Self {
            id: "ru-native".to_owned(),
            keymaps: vec![
                KeyMapActivation {
                    keymap_id: KeyMapId::new("ru-jcuken"),
                    prior_cost: 0.0,
                },
                KeyMapActivation {
                    keymap_id: KeyMapId::new("latin-qwerty"),
                    prior_cost: 4.0,
                },
            ],
            schemas: vec![
                SchemaActivation {
                    schema_id: SchemaId::new("ru-cyrillic"),
                    prior_cost: 0.0,
                    beam_budget: 32,
                },
                SchemaActivation {
                    schema_id: SchemaId::new("en-word"),
                    prior_cost: 4.0,
                    beam_budget: 4,
                },
            ],
            language_switch_cost: 2.0,
            keymap_switch_cost: 3.0,
            sentence_beam_budget: 0,
        }
    }

    pub fn ru_translit() -> Self {
        Self {
            id: "ru-translit".to_owned(),
            keymaps: vec![KeyMapActivation {
                keymap_id: KeyMapId::new("latin-qwerty"),
                prior_cost: 0.0,
            }],
            schemas: vec![
                SchemaActivation {
                    schema_id: SchemaId::new("ru-translit"),
                    prior_cost: 0.0,
                    beam_budget: 32,
                },
                SchemaActivation {
                    schema_id: SchemaId::new("en-word"),
                    prior_cost: 3.0,
                    beam_budget: 6,
                },
            ],
            language_switch_cost: 1.5,
            keymap_switch_cost: 2.0,
            sentence_beam_budget: 0,
        }
    }
}
