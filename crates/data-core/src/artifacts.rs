use std::collections::HashMap;
use std::fs::File;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::Lexicon;
#[cfg(test)]
use crate::LexiconEntry;
use crate::binary_io::*;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LanguagePackManifest {
    pub schema: String,
    #[serde(default)]
    pub kind: Option<String>,
    pub source: String,
    pub entries: usize,
    #[serde(default)]
    pub files: Vec<String>,
    #[serde(default)]
    pub components: HashMap<String, String>,
    pub format: String,
    #[serde(default)]
    pub syllables: Option<usize>,
    #[serde(default)]
    pub generated_by: Option<String>,
    #[serde(default)]
    pub generated_at_unix: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SchemaFstArtifact {
    pub format: String,
    pub schema: String,
    #[serde(default)]
    pub consumes: Option<String>,
    #[serde(default)]
    pub emits: Option<String>,
    #[serde(default)]
    pub delimiter: Option<String>,
    #[serde(default)]
    pub alphabet: Option<String>,
    #[serde(default)]
    pub syllables: Vec<String>,
    #[serde(default)]
    pub fuzzy_edges: Vec<FuzzyEdge>,
    #[serde(default)]
    pub ambiguous_compact_readings: HashMap<String, Vec<WeightedReading>>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FuzzyEdge {
    pub from: String,
    pub to: String,
    pub kind: String,
    pub cost: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WeightedReading {
    pub reading: String,
    pub weight: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FrequencyTableArtifact {
    pub format: String,
    pub schema: String,
    #[serde(default)]
    pub total_weight: f64,
    #[serde(default)]
    pub entries: usize,
    #[serde(default)]
    pub top: Vec<FrequencyEntry>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FrequencyEntry {
    pub reading: String,
    pub weight: f64,
    pub prob: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AliasTableArtifact {
    pub format: String,
    pub schema: String,
    pub source: String,
    #[serde(default)]
    pub aliases: Vec<AliasEntry>,
    #[serde(default)]
    pub personal_history: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AliasEntry {
    pub alias: String,
    pub outputs: Vec<String>,
    pub weight: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ContextModelArtifact {
    pub format: String,
    pub schema: String,
    pub unit: String,
    #[serde(default)]
    pub entries: usize,
    #[serde(default)]
    pub top: Vec<ContextPriorEntry>,
    #[serde(default)]
    pub continuations: Vec<ContextContinuationEntry>,
    #[serde(default)]
    pub use_in_energy: Option<String>,
    #[serde(default)]
    pub note: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ContextPriorEntry {
    pub reading: String,
    pub weight: f64,
    pub prob: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ContextContinuationEntry {
    pub suffix: String,
    pub reading: String,
    pub text: String,
    pub weight: f64,
    #[serde(default)]
    pub prob: f64,
}

#[derive(Clone, Debug)]
pub struct LanguagePack {
    pub root: PathBuf,
    pub manifest: LanguagePackManifest,
    pub lexicon: Lexicon,
    pub schema_fst: Option<SchemaFstArtifact>,
    pub frequency: Option<FrequencyTableArtifact>,
    pub alias_table: Option<AliasTableArtifact>,
    pub context_model: Option<ContextModelArtifact>,
    pub morphology: Option<MorphologyTableArtifact>,
    pub transliteration_table: Option<TransliterationTableArtifact>,
}

impl LanguagePack {
    pub fn load(root: impl AsRef<Path>) -> io::Result<Self> {
        let root = root.as_ref().to_path_buf();
        let manifest: LanguagePackManifest = read_manifest(root.join("manifest.json"))?;
        let lexicon_path = component_path(&root, &manifest.components, "lexicon")?;
        let lexicon = read_lexicon_component(lexicon_path)?;
        let schema_fst = optional_component(&root, &manifest.components, "schema_fst")?;
        let frequency = optional_component(&root, &manifest.components, "frequency_table")?;
        let alias_table = optional_component(&root, &manifest.components, "alias_table")?;
        let context_model = optional_component(&root, &manifest.components, "context_model")?;
        let morphology = optional_component(&root, &manifest.components, "morphology")?;
        let transliteration_table =
            optional_component(&root, &manifest.components, "transliteration_table")?;

        Ok(Self {
            root,
            manifest,
            lexicon,
            schema_fst,
            frequency,
            alias_table,
            context_model,
            morphology,
            transliteration_table,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MorphologyTableArtifact {
    pub format: String,
    pub schema: String,
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub entries: usize,
    #[serde(default)]
    pub surface_forms: Vec<MorphologyEntry>,
    #[serde(default)]
    pub use_in_energy: Option<String>,
    #[serde(default)]
    pub note: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MorphologyEntry {
    pub surface: String,
    #[serde(default)]
    pub lemma: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub weight: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TransliterationTableArtifact {
    pub format: String,
    pub schema: String,
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub rules: Vec<TransliterationRule>,
    #[serde(default)]
    pub use_in_energy: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TransliterationRule {
    pub from: String,
    pub to: String,
    #[serde(default)]
    pub cost: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ObservationPackManifest {
    pub schema: String,
    pub format: String,
    pub source: String,
    pub entries: usize,
    #[serde(default)]
    pub components: HashMap<String, String>,
    #[serde(default)]
    pub generated_by: Option<String>,
    #[serde(default)]
    pub generated_at_unix: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ObservationErrorModel {
    pub format: String,
    pub geometry: String,
    pub keymap_reference: String,
    pub source: String,
    pub samples: usize,
    pub features: HashMap<String, FeatureStats>,
    #[serde(default)]
    pub slot_errors: HashMap<String, SlotErrorStats>,
    #[serde(default)]
    pub use_in_energy: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SlotErrorStats {
    pub symbol: String,
    #[serde(default)]
    pub startpoint_error: Option<FeatureStats>,
    #[serde(default)]
    pub endpoint_error: Option<FeatureStats>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FeatureStats {
    pub count: usize,
    pub mean: f64,
    pub stdev: f64,
    pub min: f64,
    pub max: f64,
}

#[derive(Clone, Debug)]
pub struct ObservationModelPack {
    pub root: PathBuf,
    pub manifest: ObservationPackManifest,
    pub error_model: ObservationErrorModel,
    pub gesture_templates: Option<GestureTemplateArtifact>,
}

impl ObservationModelPack {
    pub fn load(root: impl AsRef<Path>) -> io::Result<Self> {
        let root = root.as_ref().to_path_buf();
        let manifest: ObservationPackManifest = read_manifest(root.join("manifest.json"))?;
        let error_model =
            read_component(component_path(&root, &manifest.components, "error_model")?)?;
        let gesture_templates =
            optional_component(&root, &manifest.components, "gesture_templates")?;
        Ok(Self {
            root,
            manifest,
            error_model,
            gesture_templates,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GestureTemplateArtifact {
    pub format: String,
    pub geometry: String,
    pub keymap_reference: String,
    #[serde(default)]
    pub point_count: usize,
    #[serde(default)]
    pub templates: Vec<GestureTemplate>,
    #[serde(default)]
    pub use_in_energy: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GestureTemplate {
    pub word: String,
    pub count: usize,
    #[serde(default)]
    pub mean_path_length: f64,
    pub points: Vec<[f64; 2]>,
}

fn component_path(
    root: &Path,
    components: &HashMap<String, String>,
    component: &str,
) -> io::Result<PathBuf> {
    components
        .get(component)
        .map(|relative| root.join(relative))
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("manifest missing required component {component}"),
            )
        })
}

pub fn read_lexicon_component(path: impl AsRef<Path>) -> io::Result<Lexicon> {
    let path = path.as_ref();
    require_bin_extension(path)?;
    // mmap the MOCHLX02 image (or transparently upgrade a legacy MOCHLX01 file).
    Lexicon::open_mmap(path)
}

fn require_bin_extension(path: &Path) -> io::Result<()> {
    if is_binary_component(path) {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("component is not binary: {}", path.display()),
        ))
    }
}

const COMPONENT_MAGIC: &[u8; 8] = b"MOCHBN01";

pub trait BinaryArtifact: Sized {
    const KIND: &'static str;

    fn write_payload(&self, writer: &mut impl Write) -> io::Result<()>;
    fn read_payload(reader: &mut impl Read) -> io::Result<Self>;
}

fn optional_component<T: BinaryArtifact>(
    root: &Path,
    components: &HashMap<String, String>,
    component: &str,
) -> io::Result<Option<T>> {
    let Some(relative) = components.get(component) else {
        return Ok(None);
    };
    match read_component(root.join(relative)) {
        Ok(value) => Ok(Some(value)),
        // An optional component can be listed in the manifest yet not shipped — e.g.
        // a release bundle that trims the (runtime-unused) FUTO gesture templates to
        // save space. A genuinely-absent optional file means "not available", NOT a
        // fatal load error, so don't let it abort the whole engine build (which would
        // crash the IME on startup). Any other IO/parse error still propagates.
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(err),
    }
}

pub fn read_component<T: BinaryArtifact>(path: impl AsRef<Path>) -> io::Result<T> {
    let path = path.as_ref();
    require_bin_extension(path)?;
    let mut reader = io::BufReader::new(File::open(path)?);
    let mut magic = [0u8; 8];
    reader.read_exact(&mut magic)?;
    if &magic != COMPONENT_MAGIC {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("invalid binary component magic in {}", path.display()),
        ));
    }
    let kind = read_string(&mut reader)?;
    if kind != T::KIND {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("binary component kind {kind} does not match {}", T::KIND),
        ));
    }
    T::read_payload(&mut reader)
}

pub fn write_component_bin<T: BinaryArtifact>(path: impl AsRef<Path>, value: &T) -> io::Result<()> {
    let mut writer = io::BufWriter::new(File::create(path)?);
    writer.write_all(COMPONENT_MAGIC)?;
    write_string(&mut writer, T::KIND)?;
    value.write_payload(&mut writer)
}

pub fn write_lexicon_bin(
    path: impl AsRef<Path>,
    _schema: impl Into<String>,
    lexicon: &Lexicon,
) -> io::Result<()> {
    // Emit the mmap-friendly MOCHLX02 image (records sorted by reading+weight).
    let bytes = crate::serialize_lexicon(lexicon.iter_entries());
    std::fs::write(path, bytes)
}

fn is_binary_component(path: &Path) -> bool {
    path.extension().and_then(|ext| ext.to_str()) == Some("bin")
}

// Little-endian binary read/write primitives moved to `crate::binary_io`.

fn read_manifest<T: for<'de> Deserialize<'de>>(path: impl AsRef<Path>) -> io::Result<T> {
    serde_json::from_reader(io::BufReader::new(File::open(path)?))
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))
}

impl BinaryArtifact for Vec<String> {
    const KIND: &'static str = "string-list-v1";

    fn write_payload(&self, writer: &mut impl Write) -> io::Result<()> {
        write_string_vec(writer, self)
    }

    fn read_payload(reader: &mut impl Read) -> io::Result<Self> {
        read_string_vec(reader)
    }
}

impl BinaryArtifact for SchemaFstArtifact {
    const KIND: &'static str = "schema-fst-v1";

    fn write_payload(&self, writer: &mut impl Write) -> io::Result<()> {
        write_string(writer, &self.format)?;
        write_string(writer, &self.schema)?;
        write_option_string(writer, &self.consumes)?;
        write_option_string(writer, &self.emits)?;
        write_option_string(writer, &self.delimiter)?;
        write_option_string(writer, &self.alphabet)?;
        write_string_vec(writer, &self.syllables)?;
        write_len(writer, self.fuzzy_edges.len())?;
        for edge in &self.fuzzy_edges {
            write_string(writer, &edge.from)?;
            write_string(writer, &edge.to)?;
            write_string(writer, &edge.kind)?;
            write_f32(writer, edge.cost)?;
        }
        let mut ambiguous = self.ambiguous_compact_readings.iter().collect::<Vec<_>>();
        ambiguous.sort_by(|a, b| a.0.cmp(b.0));
        write_len(writer, ambiguous.len())?;
        for (compact, readings) in ambiguous {
            write_string(writer, compact)?;
            write_len(writer, readings.len())?;
            for reading in readings {
                write_string(writer, &reading.reading)?;
                write_f32(writer, reading.weight)?;
            }
        }
        Ok(())
    }

    fn read_payload(reader: &mut impl Read) -> io::Result<Self> {
        let format = read_string(reader)?;
        let schema = read_string(reader)?;
        let consumes = read_option_string(reader)?;
        let emits = read_option_string(reader)?;
        let delimiter = read_option_string(reader)?;
        let alphabet = read_option_string(reader)?;
        let syllables = read_string_vec(reader)?;
        let fuzzy_edges = (0..read_len(reader)?)
            .map(|_| {
                Ok(FuzzyEdge {
                    from: read_string(reader)?,
                    to: read_string(reader)?,
                    kind: read_string(reader)?,
                    cost: read_f32(reader)?,
                })
            })
            .collect::<io::Result<Vec<_>>>()?;
        let mut ambiguous_compact_readings = HashMap::new();
        for _ in 0..read_len(reader)? {
            let compact = read_string(reader)?;
            let readings = (0..read_len(reader)?)
                .map(|_| {
                    Ok(WeightedReading {
                        reading: read_string(reader)?,
                        weight: read_f32(reader)?,
                    })
                })
                .collect::<io::Result<Vec<_>>>()?;
            ambiguous_compact_readings.insert(compact, readings);
        }
        Ok(Self {
            format,
            schema,
            consumes,
            emits,
            delimiter,
            alphabet,
            syllables,
            fuzzy_edges,
            ambiguous_compact_readings,
        })
    }
}

impl BinaryArtifact for FrequencyTableArtifact {
    const KIND: &'static str = "frequency-table-v1";

    fn write_payload(&self, writer: &mut impl Write) -> io::Result<()> {
        write_string(writer, &self.format)?;
        write_string(writer, &self.schema)?;
        write_f64(writer, self.total_weight)?;
        write_len(writer, self.entries)?;
        write_len(writer, self.top.len())?;
        for entry in &self.top {
            write_string(writer, &entry.reading)?;
            write_f64(writer, entry.weight)?;
            write_f64(writer, entry.prob)?;
        }
        Ok(())
    }

    fn read_payload(reader: &mut impl Read) -> io::Result<Self> {
        let format = read_string(reader)?;
        let schema = read_string(reader)?;
        let total_weight = read_f64(reader)?;
        let entries = read_len(reader)?;
        let top = (0..read_len(reader)?)
            .map(|_| {
                Ok(FrequencyEntry {
                    reading: read_string(reader)?,
                    weight: read_f64(reader)?,
                    prob: read_f64(reader)?,
                })
            })
            .collect::<io::Result<Vec<_>>>()?;
        Ok(Self {
            format,
            schema,
            total_weight,
            entries,
            top,
        })
    }
}

impl BinaryArtifact for AliasTableArtifact {
    const KIND: &'static str = "alias-table-v1";

    fn write_payload(&self, writer: &mut impl Write) -> io::Result<()> {
        write_string(writer, &self.format)?;
        write_string(writer, &self.schema)?;
        write_string(writer, &self.source)?;
        write_len(writer, self.aliases.len())?;
        for entry in &self.aliases {
            write_string(writer, &entry.alias)?;
            write_string_vec(writer, &entry.outputs)?;
            write_f32(writer, entry.weight)?;
        }
        write_option_string(writer, &self.personal_history)
    }

    fn read_payload(reader: &mut impl Read) -> io::Result<Self> {
        let format = read_string(reader)?;
        let schema = read_string(reader)?;
        let source = read_string(reader)?;
        let aliases = (0..read_len(reader)?)
            .map(|_| {
                Ok(AliasEntry {
                    alias: read_string(reader)?,
                    outputs: read_string_vec(reader)?,
                    weight: read_f32(reader)?,
                })
            })
            .collect::<io::Result<Vec<_>>>()?;
        let personal_history = read_option_string(reader)?;
        Ok(Self {
            format,
            schema,
            source,
            aliases,
            personal_history,
        })
    }
}

impl BinaryArtifact for ContextModelArtifact {
    const KIND: &'static str = "context-model-v1";

    fn write_payload(&self, writer: &mut impl Write) -> io::Result<()> {
        write_string(writer, &self.format)?;
        write_string(writer, &self.schema)?;
        write_string(writer, &self.unit)?;
        write_len(writer, self.entries)?;
        write_len(writer, self.top.len())?;
        for entry in &self.top {
            write_string(writer, &entry.reading)?;
            write_f64(writer, entry.weight)?;
            write_f64(writer, entry.prob)?;
        }
        write_len(writer, self.continuations.len())?;
        for entry in &self.continuations {
            write_string(writer, &entry.suffix)?;
            write_string(writer, &entry.reading)?;
            write_string(writer, &entry.text)?;
            write_f64(writer, entry.weight)?;
            write_f64(writer, entry.prob)?;
        }
        write_option_string(writer, &self.use_in_energy)?;
        write_option_string(writer, &self.note)
    }

    fn read_payload(reader: &mut impl Read) -> io::Result<Self> {
        let format = read_string(reader)?;
        let schema = read_string(reader)?;
        let unit = read_string(reader)?;
        let entries = read_len(reader)?;
        let top = (0..read_len(reader)?)
            .map(|_| {
                Ok(ContextPriorEntry {
                    reading: read_string(reader)?,
                    weight: read_f64(reader)?,
                    prob: read_f64(reader)?,
                })
            })
            .collect::<io::Result<Vec<_>>>()?;
        let continuations = (0..read_len(reader)?)
            .map(|_| {
                Ok(ContextContinuationEntry {
                    suffix: read_string(reader)?,
                    reading: read_string(reader)?,
                    text: read_string(reader)?,
                    weight: read_f64(reader)?,
                    prob: read_f64(reader)?,
                })
            })
            .collect::<io::Result<Vec<_>>>()?;
        let use_in_energy = read_option_string(reader)?;
        let note = read_option_string(reader)?;
        Ok(Self {
            format,
            schema,
            unit,
            entries,
            top,
            continuations,
            use_in_energy,
            note,
        })
    }
}

impl BinaryArtifact for MorphologyTableArtifact {
    const KIND: &'static str = "morphology-table-v1";

    fn write_payload(&self, writer: &mut impl Write) -> io::Result<()> {
        write_string(writer, &self.format)?;
        write_string(writer, &self.schema)?;
        write_string(writer, &self.source)?;
        write_len(writer, self.entries)?;
        write_len(writer, self.surface_forms.len())?;
        for entry in &self.surface_forms {
            write_string(writer, &entry.surface)?;
            write_option_string(writer, &entry.lemma)?;
            write_string_vec(writer, &entry.tags)?;
            write_f64(writer, entry.weight)?;
        }
        write_option_string(writer, &self.use_in_energy)?;
        write_option_string(writer, &self.note)
    }

    fn read_payload(reader: &mut impl Read) -> io::Result<Self> {
        let format = read_string(reader)?;
        let schema = read_string(reader)?;
        let source = read_string(reader)?;
        let entries = read_len(reader)?;
        let surface_forms = (0..read_len(reader)?)
            .map(|_| {
                Ok(MorphologyEntry {
                    surface: read_string(reader)?,
                    lemma: read_option_string(reader)?,
                    tags: read_string_vec(reader)?,
                    weight: read_f64(reader)?,
                })
            })
            .collect::<io::Result<Vec<_>>>()?;
        let use_in_energy = read_option_string(reader)?;
        let note = read_option_string(reader)?;
        Ok(Self {
            format,
            schema,
            source,
            entries,
            surface_forms,
            use_in_energy,
            note,
        })
    }
}

impl BinaryArtifact for TransliterationTableArtifact {
    const KIND: &'static str = "transliteration-table-v1";

    fn write_payload(&self, writer: &mut impl Write) -> io::Result<()> {
        write_string(writer, &self.format)?;
        write_string(writer, &self.schema)?;
        write_string(writer, &self.source)?;
        write_len(writer, self.rules.len())?;
        for rule in &self.rules {
            write_string(writer, &rule.from)?;
            write_string(writer, &rule.to)?;
            write_f32(writer, rule.cost)?;
        }
        write_option_string(writer, &self.use_in_energy)
    }

    fn read_payload(reader: &mut impl Read) -> io::Result<Self> {
        let format = read_string(reader)?;
        let schema = read_string(reader)?;
        let source = read_string(reader)?;
        let rules = (0..read_len(reader)?)
            .map(|_| {
                Ok(TransliterationRule {
                    from: read_string(reader)?,
                    to: read_string(reader)?,
                    cost: read_f32(reader)?,
                })
            })
            .collect::<io::Result<Vec<_>>>()?;
        let use_in_energy = read_option_string(reader)?;
        Ok(Self {
            format,
            schema,
            source,
            rules,
            use_in_energy,
        })
    }
}

impl BinaryArtifact for ObservationErrorModel {
    const KIND: &'static str = "observation-error-model-v1";

    fn write_payload(&self, writer: &mut impl Write) -> io::Result<()> {
        write_string(writer, &self.format)?;
        write_string(writer, &self.geometry)?;
        write_string(writer, &self.keymap_reference)?;
        write_string(writer, &self.source)?;
        write_len(writer, self.samples)?;
        let mut features = self.features.iter().collect::<Vec<_>>();
        features.sort_by(|a, b| a.0.cmp(b.0));
        write_len(writer, features.len())?;
        for (name, stats) in features {
            write_string(writer, name)?;
            write_feature_stats(writer, stats)?;
        }
        let mut slot_errors = self.slot_errors.iter().collect::<Vec<_>>();
        slot_errors.sort_by(|a, b| a.0.cmp(b.0));
        write_len(writer, slot_errors.len())?;
        for (slot, stats) in slot_errors {
            write_string(writer, slot)?;
            write_string(writer, &stats.symbol)?;
            write_option_feature_stats(writer, &stats.startpoint_error)?;
            write_option_feature_stats(writer, &stats.endpoint_error)?;
        }
        write_option_string(writer, &self.use_in_energy)
    }

    fn read_payload(reader: &mut impl Read) -> io::Result<Self> {
        let format = read_string(reader)?;
        let geometry = read_string(reader)?;
        let keymap_reference = read_string(reader)?;
        let source = read_string(reader)?;
        let samples = read_len(reader)?;
        let mut features = HashMap::new();
        for _ in 0..read_len(reader)? {
            features.insert(read_string(reader)?, read_feature_stats(reader)?);
        }
        let mut slot_errors = HashMap::new();
        for _ in 0..read_len(reader)? {
            let slot = read_string(reader)?;
            slot_errors.insert(
                slot,
                SlotErrorStats {
                    symbol: read_string(reader)?,
                    startpoint_error: read_option_feature_stats(reader)?,
                    endpoint_error: read_option_feature_stats(reader)?,
                },
            );
        }
        let use_in_energy = read_option_string(reader)?;
        Ok(Self {
            format,
            geometry,
            keymap_reference,
            source,
            samples,
            features,
            slot_errors,
            use_in_energy,
        })
    }
}

impl BinaryArtifact for GestureTemplateArtifact {
    const KIND: &'static str = "gesture-templates-v1";

    fn write_payload(&self, writer: &mut impl Write) -> io::Result<()> {
        write_string(writer, &self.format)?;
        write_string(writer, &self.geometry)?;
        write_string(writer, &self.keymap_reference)?;
        write_len(writer, self.point_count)?;
        write_len(writer, self.templates.len())?;
        for template in &self.templates {
            write_string(writer, &template.word)?;
            write_len(writer, template.count)?;
            write_f64(writer, template.mean_path_length)?;
            write_len(writer, template.points.len())?;
            for point in &template.points {
                write_f64(writer, point[0])?;
                write_f64(writer, point[1])?;
            }
        }
        write_option_string(writer, &self.use_in_energy)
    }

    fn read_payload(reader: &mut impl Read) -> io::Result<Self> {
        let format = read_string(reader)?;
        let geometry = read_string(reader)?;
        let keymap_reference = read_string(reader)?;
        let point_count = read_len(reader)?;
        let templates = (0..read_len(reader)?)
            .map(|_| {
                let word = read_string(reader)?;
                let count = read_len(reader)?;
                let mean_path_length = read_f64(reader)?;
                let points = (0..read_len(reader)?)
                    .map(|_| Ok([read_f64(reader)?, read_f64(reader)?]))
                    .collect::<io::Result<Vec<_>>>()?;
                Ok(GestureTemplate {
                    word,
                    count,
                    mean_path_length,
                    points,
                })
            })
            .collect::<io::Result<Vec<_>>>()?;
        let use_in_energy = read_option_string(reader)?;
        Ok(Self {
            format,
            geometry,
            keymap_reference,
            point_count,
            templates,
            use_in_energy,
        })
    }
}

fn write_feature_stats(writer: &mut impl Write, stats: &FeatureStats) -> io::Result<()> {
    write_len(writer, stats.count)?;
    write_f64(writer, stats.mean)?;
    write_f64(writer, stats.stdev)?;
    write_f64(writer, stats.min)?;
    write_f64(writer, stats.max)
}

fn read_feature_stats(reader: &mut impl Read) -> io::Result<FeatureStats> {
    Ok(FeatureStats {
        count: read_len(reader)?,
        mean: read_f64(reader)?,
        stdev: read_f64(reader)?,
        min: read_f64(reader)?,
        max: read_f64(reader)?,
    })
}

fn write_option_feature_stats(
    writer: &mut impl Write,
    stats: &Option<FeatureStats>,
) -> io::Result<()> {
    write_bool(writer, stats.is_some())?;
    if let Some(stats) = stats {
        write_feature_stats(writer, stats)?;
    }
    Ok(())
}

fn read_option_feature_stats(reader: &mut impl Read) -> io::Result<Option<FeatureStats>> {
    if read_bool(reader)? {
        read_feature_stats(reader).map(Some)
    } else {
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn flat_lexicon_round_trips() {
        let path = temp_path("lexicon.bin");
        let lexicon = Lexicon::new([
            LexiconEntry::new("西安", "xi an", 10.0),
            LexiconEntry::new("先", "xian", 5.0),
        ]);
        write_lexicon_bin(&path, "zh-hans-pinyin-full", &lexicon).unwrap();
        let loaded = read_lexicon_component(&path).unwrap();
        assert_eq!(loaded.lookup_reading("xi an")[0].text, "西安");
        assert_eq!(loaded.lookup_reading("xian")[0].text, "先");
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn schema_fst_component_round_trips() {
        let path = temp_path("schema_fst.bin");
        let mut ambiguous = HashMap::new();
        ambiguous.insert(
            "xian".to_owned(),
            vec![
                WeightedReading {
                    reading: "xian".to_owned(),
                    weight: 10.0,
                },
                WeightedReading {
                    reading: "xi an".to_owned(),
                    weight: 5.0,
                },
            ],
        );
        let fst = SchemaFstArtifact {
            format: "mocha-schema-fst-v1".to_owned(),
            schema: "zh-hans-pinyin-full".to_owned(),
            consumes: None,
            emits: None,
            delimiter: Some("'".to_owned()),
            alphabet: None,
            syllables: vec!["xi".to_owned(), "an".to_owned(), "xian".to_owned()],
            fuzzy_edges: vec![FuzzyEdge {
                from: "en".to_owned(),
                to: "eng".to_owned(),
                kind: "fuzzy".to_owned(),
                cost: 1.0,
            }],
            ambiguous_compact_readings: ambiguous,
        };
        write_component_bin(&path, &fst).unwrap();
        let loaded: SchemaFstArtifact = read_component(&path).unwrap();
        assert_eq!(loaded.syllables.len(), 3);
        assert_eq!(
            loaded.ambiguous_compact_readings["xian"][1].reading,
            "xi an"
        );
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn context_model_component_round_trips() {
        let path = temp_path("context_model.bin");
        let model = ContextModelArtifact {
            format: "mocha-context-model-v1".to_owned(),
            schema: "en-word".to_owned(),
            unit: "word".to_owned(),
            entries: 1,
            top: vec![ContextPriorEntry {
                reading: "hello".to_owned(),
                weight: 1.0,
                prob: 1.0,
            }],
            continuations: vec![ContextContinuationEntry {
                suffix: "i".to_owned(),
                reading: "cannot".to_owned(),
                text: "cannot".to_owned(),
                weight: 4.0,
                prob: 0.8,
            }],
            use_in_energy: Some("E_context(x, r, b, z, C)".to_owned()),
            note: None,
        };
        write_component_bin(&path, &model).unwrap();
        let loaded: ContextModelArtifact = read_component(&path).unwrap();
        assert_eq!(loaded.top[0].reading, "hello");
        assert_eq!(loaded.continuations[0].suffix, "i");
        assert_eq!(loaded.continuations[0].text, "cannot");
        assert_eq!(
            loaded.use_in_energy.as_deref(),
            Some("E_context(x, r, b, z, C)")
        );
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn gesture_template_component_round_trips() {
        let path = temp_path("gesture_templates.bin");
        let templates = GestureTemplateArtifact {
            format: "mocha-gesture-templates-v1".to_owned(),
            geometry: "geometry-phone-10col".to_owned(),
            keymap_reference: "latin-qwerty".to_owned(),
            point_count: 2,
            templates: vec![GestureTemplate {
                word: "the".to_owned(),
                count: 3,
                mean_path_length: 0.5,
                points: vec![[0.1, 0.2], [0.3, 0.4]],
            }],
            use_in_energy: Some("E_obs(O, q, G)".to_owned()),
        };
        write_component_bin(&path, &templates).unwrap();
        let loaded: GestureTemplateArtifact = read_component(&path).unwrap();
        assert_eq!(loaded.templates[0].word, "the");
        assert_eq!(loaded.templates[0].points.len(), 2);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn observation_pack_loads_without_optional_gesture_templates() {
        // A trimmed release bundle drops the runtime-unused gesture_templates.bin,
        // yet the manifest still references it. Loading MUST succeed with
        // templates = None — otherwise the engine never builds and the IME crashes
        // on first load (regression: APK asset trimming → "failed to load zh engine:
        // No such file or directory").
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("mocha-obs-{nanos}"));
        std::fs::create_dir_all(&dir).unwrap();

        let error_model = ObservationErrorModel {
            format: "mocha-observation-error-model-v1".to_owned(),
            geometry: "geometry-phone-10col".to_owned(),
            keymap_reference: "latin-qwerty".to_owned(),
            source: "test".to_owned(),
            samples: 1,
            features: HashMap::new(),
            slot_errors: HashMap::new(),
            use_in_energy: None,
        };
        let error_model_path = dir.join("error_model.bin");
        write_component_bin(&error_model_path, &error_model).unwrap();

        // References gesture_templates.bin, which we deliberately do NOT write.
        let manifest = r#"{
            "schema": "observation",
            "format": "mocha-observation-pack-v1",
            "source": "test",
            "entries": 1,
            "components": {
                "gesture_templates": "gesture_templates.bin",
                "error_model": "error_model.bin"
            }
        }"#;
        std::fs::write(dir.join("manifest.json"), manifest).unwrap();

        let pack =
            ObservationModelPack::load(&dir).expect("load must tolerate a missing optional file");
        assert!(pack.gesture_templates.is_none());

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn component_kind_mismatch_is_rejected() {
        let path = temp_path("frequency.bin");
        let table = FrequencyTableArtifact {
            format: "mocha-frequency-table-v1".to_owned(),
            schema: "en-word".to_owned(),
            total_weight: 1.0,
            entries: 1,
            top: vec![FrequencyEntry {
                reading: "hello".to_owned(),
                weight: 1.0,
                prob: 1.0,
            }],
        };
        write_component_bin(&path, &table).unwrap();
        let err = read_component::<ContextModelArtifact>(&path).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
        std::fs::remove_file(path).unwrap();
    }

    fn temp_path(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("mocha-{nanos}-{name}"))
    }
}
