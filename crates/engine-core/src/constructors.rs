use std::collections::HashMap;
use std::io;
use std::path::Path;

use context_core::{ContextCompiler, TextContext};
use data_core::{LanguagePack, Lexicon, ObservationModelPack};
use decoder_beam::BeamDecoder;
use geometry_phone_10col::Phone10ColGeometry;
use keymap_latin_qwerty::LatinQwertyKeyMap;
use keymap_latin_spanish::LatinSpanishKeyMap;
use keymap_ru_jcuken::RuJcukenKeyMap;
use lm_core::NullLanguageModel;
use personal_core::PersonalKnowledgeStore;
use profile_core::KeyboardProfile;
use schema_emoji::EmojiSchema;
use schema_en_word::EnglishWordSchema;
use schema_es_word::SpanishWordSchema;
use schema_ru_cyrillic::RuCyrillicSchema;
use schema_ru_translit::RuTranslitSchema;
use schema_zh_hans_pinyin_full::ZhHansPinyinFullSchema;

use crate::{
    ImeEngine, context_cache_from_language_packs, emoji_aliases_from_pack,
    gesture_templates_from_pack, observation_distance_unit_from_pack,
    observation_slot_units_from_pack, read_optional_emoji_pack,
};

impl ImeEngine {
    pub fn zh_qwerty_demo() -> Self {
        Self::zh_qwerty_with_lexicons(Lexicon::new([]), Lexicon::new([]), true)
    }

    pub fn zh_qwerty_from_language_packs(root: impl AsRef<Path>) -> io::Result<Self> {
        let root = root.as_ref();
        let pinyin = LanguagePack::load(root.join("zh-hans-pinyin-full"))?;
        let english = LanguagePack::load(root.join("en-word"))?;
        let pinyin_fst = pinyin.schema_fst.clone();
        let syllables = pinyin_fst
            .as_ref()
            .map(|fst| fst.syllables.clone())
            .unwrap_or_default();
        let ambiguous_compact_readings = pinyin_fst
            .as_ref()
            .map(|fst| fst.ambiguous_compact_readings.clone())
            .unwrap_or_default();
        let emoji_pack = read_optional_emoji_pack(root, "emoji-zh-hans")?;
        let emoji_aliases = emoji_pack.as_ref().and_then(emoji_aliases_from_pack);
        let mut context_packs = vec![&pinyin, &english];
        if let Some(pack) = emoji_pack.as_ref() {
            context_packs.push(pack);
        }
        let context_compiler =
            ContextCompiler::with_base_cache(context_cache_from_language_packs(&context_packs));
        Ok(Self::zh_qwerty_with_lexicon_data(
            pinyin.lexicon,
            english.lexicon,
            false,
            Some(syllables),
            Some(ambiguous_compact_readings),
            emoji_aliases,
            Some(context_compiler),
            None,
        ))
    }

    pub fn zh_qwerty_from_artifacts(
        language_pack_root: impl AsRef<Path>,
        observation_pack_root: impl AsRef<Path>,
    ) -> io::Result<Self> {
        let mut engine = Self::zh_qwerty_from_language_packs(language_pack_root)?;
        let observation_pack = ObservationModelPack::load(observation_pack_root)?;
        if let Some(unit) = observation_distance_unit_from_pack(&observation_pack) {
            engine.observation_distance_unit = unit;
        }
        engine.observation_slot_units = observation_slot_units_from_pack(&observation_pack);
        engine.install_gesture_templates(gesture_templates_from_pack(&observation_pack));
        Ok(engine)
    }

    pub fn en_qwerty_from_language_packs(root: impl AsRef<Path>) -> io::Result<Self> {
        let root = root.as_ref();
        let english = LanguagePack::load(root.join("en-word"))?;
        let emoji_pack = read_optional_emoji_pack(root, "emoji-en")?;
        let emoji_aliases = emoji_pack.as_ref().and_then(emoji_aliases_from_pack);
        let mut context_packs = vec![&english];
        if let Some(pack) = emoji_pack.as_ref() {
            context_packs.push(pack);
        }
        let context_compiler =
            ContextCompiler::with_base_cache(context_cache_from_language_packs(&context_packs));
        let emoji_schema = emoji_aliases
            .map(|aliases| EmojiSchema::new("emoji-en", aliases))
            .unwrap_or_else(EmojiSchema::builtin_en);
        Ok(Self {
            geometry: Box::<Phone10ColGeometry>::default(),
            keymaps: vec![Box::<LatinQwertyKeyMap>::default()],
            schemas: vec![
                Box::new(EnglishWordSchema::new(english.lexicon)),
                Box::new(emoji_schema),
            ],
            profile: KeyboardProfile::en_qwerty(),
            context_compiler,
            context: TextContext::default(),
            personal: PersonalKnowledgeStore::default().compile(),
            decoder: BeamDecoder::default(),
            lm: Box::new(NullLanguageModel),
            observation_distance_unit: 1.0,
            observation_slot_units: HashMap::new(),
            gesture_templates: Vec::new(),
            gesture_start_index: crate::gesture::GestureTemplateStartIndex::default(),
            pending_stream_points: Vec::new(),
            pending_gesture_points: Vec::new(),
            pending_stream_pause_positions: Vec::new(),
            pending_stream_last_slot: None,
            pending_stream_candidates: decoder_core::CandidateList::default(),
            pending_gesture_scored_stream_len: 0,
            swipe_lm_session: lm_core::LmSession::new(),
            swipe_accepted_text: String::new(),
            expert_lms: HashMap::new(),
        })
    }

    pub fn en_qwerty_from_artifacts(
        language_pack_root: impl AsRef<Path>,
        observation_pack_root: impl AsRef<Path>,
    ) -> io::Result<Self> {
        let mut engine = Self::en_qwerty_from_language_packs(language_pack_root)?;
        let observation_pack = ObservationModelPack::load(observation_pack_root)?;
        if let Some(unit) = observation_distance_unit_from_pack(&observation_pack) {
            engine.observation_distance_unit = unit;
        }
        engine.observation_slot_units = observation_slot_units_from_pack(&observation_pack);
        engine.install_gesture_templates(gesture_templates_from_pack(&observation_pack));
        Ok(engine)
    }

    pub fn en_es_qwerty_demo() -> Self {
        Self {
            geometry: Box::<Phone10ColGeometry>::default(),
            keymaps: vec![Box::<LatinSpanishKeyMap>::default()],
            schemas: vec![
                Box::<EnglishWordSchema>::default(),
                Box::<SpanishWordSchema>::default(),
                Box::new(EmojiSchema::builtin_en()),
            ],
            profile: KeyboardProfile::en_es_qwerty(),
            context_compiler: ContextCompiler::default(),
            context: TextContext::default(),
            personal: PersonalKnowledgeStore::default().compile(),
            decoder: BeamDecoder::default(),
            lm: Box::new(NullLanguageModel),
            observation_distance_unit: 1.0,
            observation_slot_units: HashMap::new(),
            gesture_templates: Vec::new(),
            gesture_start_index: crate::gesture::GestureTemplateStartIndex::default(),
            pending_stream_points: Vec::new(),
            pending_gesture_points: Vec::new(),
            pending_stream_pause_positions: Vec::new(),
            pending_stream_last_slot: None,
            pending_stream_candidates: decoder_core::CandidateList::default(),
            pending_gesture_scored_stream_len: 0,
            swipe_lm_session: lm_core::LmSession::new(),
            swipe_accepted_text: String::new(),
            expert_lms: HashMap::new(),
        }
    }

    pub fn ru_native_demo() -> Self {
        Self {
            geometry: Box::<Phone10ColGeometry>::default(),
            keymaps: vec![
                Box::<RuJcukenKeyMap>::default(),
                Box::<LatinQwertyKeyMap>::default(),
            ],
            schemas: vec![
                Box::<RuCyrillicSchema>::default(),
                Box::<EnglishWordSchema>::default(),
            ],
            profile: KeyboardProfile::ru_native(),
            context_compiler: ContextCompiler::default(),
            context: TextContext::default(),
            personal: PersonalKnowledgeStore::default().compile(),
            decoder: BeamDecoder::default(),
            lm: Box::new(NullLanguageModel),
            observation_distance_unit: 1.0,
            observation_slot_units: HashMap::new(),
            gesture_templates: Vec::new(),
            gesture_start_index: crate::gesture::GestureTemplateStartIndex::default(),
            pending_stream_points: Vec::new(),
            pending_gesture_points: Vec::new(),
            pending_stream_pause_positions: Vec::new(),
            pending_stream_last_slot: None,
            pending_stream_candidates: decoder_core::CandidateList::default(),
            pending_gesture_scored_stream_len: 0,
            swipe_lm_session: lm_core::LmSession::new(),
            swipe_accepted_text: String::new(),
            expert_lms: HashMap::new(),
        }
    }

    pub fn ru_translit_demo() -> Self {
        Self {
            geometry: Box::<Phone10ColGeometry>::default(),
            keymaps: vec![Box::<LatinQwertyKeyMap>::default()],
            schemas: vec![
                Box::<RuTranslitSchema>::default(),
                Box::<EnglishWordSchema>::default(),
            ],
            profile: KeyboardProfile::ru_translit(),
            context_compiler: ContextCompiler::default(),
            context: TextContext::default(),
            personal: PersonalKnowledgeStore::default().compile(),
            decoder: BeamDecoder::default(),
            lm: Box::new(NullLanguageModel),
            observation_distance_unit: 1.0,
            observation_slot_units: HashMap::new(),
            gesture_templates: Vec::new(),
            gesture_start_index: crate::gesture::GestureTemplateStartIndex::default(),
            pending_stream_points: Vec::new(),
            pending_gesture_points: Vec::new(),
            pending_stream_pause_positions: Vec::new(),
            pending_stream_last_slot: None,
            pending_stream_candidates: decoder_core::CandidateList::default(),
            pending_gesture_scored_stream_len: 0,
            swipe_lm_session: lm_core::LmSession::new(),
            swipe_accepted_text: String::new(),
            expert_lms: HashMap::new(),
        }
    }

    pub(crate) fn zh_qwerty_with_lexicons(
        pinyin: Lexicon,
        english: Lexicon,
        use_builtin: bool,
    ) -> Self {
        Self::zh_qwerty_with_lexicon_data(
            pinyin,
            english,
            use_builtin,
            None,
            None,
            None,
            None,
            None,
        )
    }

    fn zh_qwerty_with_lexicon_data(
        pinyin: Lexicon,
        english: Lexicon,
        use_builtin: bool,
        pinyin_syllables: Option<Vec<String>>,
        pinyin_ambiguous_readings: Option<HashMap<String, Vec<data_core::WeightedReading>>>,
        emoji_aliases: Option<HashMap<String, Vec<String>>>,
        context_compiler: Option<ContextCompiler>,
        observation_distance_unit: Option<f32>,
    ) -> Self {
        let pinyin_schema = if use_builtin {
            ZhHansPinyinFullSchema::builtin()
        } else if let Some(syllables) = pinyin_syllables {
            ZhHansPinyinFullSchema::with_syllables_and_ambiguous_readings(
                pinyin,
                syllables,
                pinyin_ambiguous_readings.unwrap_or_default(),
            )
        } else {
            ZhHansPinyinFullSchema::new(pinyin)
        };
        let english_schema = if use_builtin {
            EnglishWordSchema::builtin()
        } else {
            EnglishWordSchema::new(english)
        };
        let emoji_schema = emoji_aliases
            .map(|aliases| EmojiSchema::new("emoji-zh-hans", aliases))
            .unwrap_or_else(EmojiSchema::builtin_zh_hans);
        Self {
            geometry: Box::<Phone10ColGeometry>::default(),
            keymaps: vec![Box::<LatinQwertyKeyMap>::default()],
            schemas: vec![
                Box::new(pinyin_schema),
                Box::new(english_schema),
                Box::new(emoji_schema),
            ],
            profile: KeyboardProfile::zh_qwerty(),
            context_compiler: context_compiler.unwrap_or_default(),
            context: TextContext::default(),
            personal: PersonalKnowledgeStore::default().compile(),
            decoder: BeamDecoder::default(),
            lm: Box::new(NullLanguageModel),
            observation_distance_unit: observation_distance_unit.unwrap_or(1.0),
            observation_slot_units: HashMap::new(),
            gesture_templates: Vec::new(),
            gesture_start_index: crate::gesture::GestureTemplateStartIndex::default(),
            pending_stream_points: Vec::new(),
            pending_gesture_points: Vec::new(),
            pending_stream_pause_positions: Vec::new(),
            pending_stream_last_slot: None,
            pending_stream_candidates: decoder_core::CandidateList::default(),
            pending_gesture_scored_stream_len: 0,
            swipe_lm_session: lm_core::LmSession::new(),
            swipe_accepted_text: String::new(),
            expert_lms: HashMap::new(),
        }
    }
}
