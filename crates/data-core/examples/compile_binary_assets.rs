use std::collections::HashMap;
use std::error::Error;
use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};

use data_core::{
    AliasTableArtifact, BinaryArtifact, ContextModelArtifact, FrequencyTableArtifact,
    GestureTemplateArtifact, LanguagePackManifest, Lexicon, MorphologyTableArtifact,
    ObservationErrorModel, ObservationPackManifest, SchemaFstArtifact,
    TransliterationTableArtifact, write_component_bin, write_lexicon_bin,
};
use serde::Deserialize;

fn main() -> Result<(), Box<dyn Error>> {
    let registry_path = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("assets/artifact-registry.json"));
    let keep_json = std::env::args().any(|arg| arg == "--keep-json");
    let registry: ArtifactRegistry = serde_json::from_reader(File::open(&registry_path)?)?;

    for entry in registry.language_packs {
        if entry.status == "ready" && entry.has_pack {
            compile_language_pack(&entry.language_pack, keep_json)?;
        }
    }
    for entry in registry.observation_models {
        if entry.status == "ready" {
            compile_observation_pack(&entry.path, keep_json)?;
        }
    }

    Ok(())
}

fn compile_language_pack(root: &Path, keep_json: bool) -> Result<(), Box<dyn Error>> {
    let manifest_path = root.join("manifest.json");
    let mut manifest: LanguagePackManifest = serde_json::from_reader(File::open(&manifest_path)?)?;
    compile_lexicon(root, &mut manifest.components, &manifest.schema, keep_json)?;
    compile_json_component::<SchemaFstArtifact>(
        root,
        &mut manifest.components,
        "schema_fst",
        keep_json,
    )?;
    compile_json_component::<FrequencyTableArtifact>(
        root,
        &mut manifest.components,
        "frequency_table",
        keep_json,
    )?;
    compile_json_component::<AliasTableArtifact>(
        root,
        &mut manifest.components,
        "alias_table",
        keep_json,
    )?;
    compile_json_component::<ContextModelArtifact>(
        root,
        &mut manifest.components,
        "context_model",
        keep_json,
    )?;
    compile_json_component::<MorphologyTableArtifact>(
        root,
        &mut manifest.components,
        "morphology",
        keep_json,
    )?;
    compile_json_component::<TransliterationTableArtifact>(
        root,
        &mut manifest.components,
        "transliteration_table",
        keep_json,
    )?;
    compile_json_component::<Vec<String>>(root, &mut manifest.components, "syllables", keep_json)?;
    write_json(&manifest_path, &manifest)?;
    Ok(())
}

fn compile_observation_pack(root: &Path, keep_json: bool) -> Result<(), Box<dyn Error>> {
    let manifest_path = root.join("manifest.json");
    let mut manifest: ObservationPackManifest =
        serde_json::from_reader(File::open(&manifest_path)?)?;
    compile_json_component::<ObservationErrorModel>(
        root,
        &mut manifest.components,
        "error_model",
        keep_json,
    )?;
    compile_json_component::<GestureTemplateArtifact>(
        root,
        &mut manifest.components,
        "gesture_templates",
        keep_json,
    )?;
    write_json(&manifest_path, &manifest)?;
    Ok(())
}

fn compile_lexicon(
    root: &Path,
    components: &mut HashMap<String, String>,
    schema: &str,
    keep_json: bool,
) -> Result<(), Box<dyn Error>> {
    let Some(relative) = components.get("lexicon").cloned() else {
        return Ok(());
    };
    let source = root.join(&relative);
    if source.extension().and_then(|ext| ext.to_str()) == Some("bin") {
        return Ok(());
    }
    let lexicon = read_training_lexicon_jsonl(&source)?;
    let target_relative = "lexicon.bin".to_owned();
    let target = root.join(&target_relative);
    write_lexicon_bin(&target, schema, &lexicon)?;
    components.insert("lexicon".to_owned(), target_relative);
    remove_source(&source, keep_json)?;
    Ok(())
}

fn compile_json_component<T>(
    root: &Path,
    components: &mut HashMap<String, String>,
    name: &str,
    keep_json: bool,
) -> Result<(), Box<dyn Error>>
where
    T: BinaryArtifact + for<'de> serde::Deserialize<'de>,
{
    let Some(relative) = components.get(name).cloned() else {
        return Ok(());
    };
    let source = root.join(&relative);
    if source.extension().and_then(|ext| ext.to_str()) == Some("bin") {
        return Ok(());
    }
    let artifact: T = serde_json::from_reader(File::open(&source)?)?;
    let target_relative = format!("{name}.bin");
    let target = root.join(&target_relative);
    write_component_bin(&target, &artifact)?;
    components.insert(name.to_owned(), target_relative);
    remove_source(&source, keep_json)?;
    Ok(())
}

fn remove_source(path: &Path, keep_json: bool) -> io::Result<()> {
    if !keep_json && path.exists() {
        fs::remove_file(path)?;
    }
    Ok(())
}

fn write_json<T: serde::Serialize>(path: &Path, value: &T) -> io::Result<()> {
    serde_json::to_writer_pretty(File::create(path)?, value)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))
}

fn read_training_lexicon_jsonl(path: &Path) -> io::Result<Lexicon> {
    let reader = io::BufReader::new(File::open(path)?);
    let mut entries = Vec::new();
    for line in io::BufRead::lines(reader) {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let parsed: TrainingLexiconEntry = serde_json::from_str(&line)
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
        entries.push(data_core::LexiconEntry::new(
            parsed.text,
            parsed.reading,
            parsed.weight,
        ));
    }
    Ok(Lexicon::new(entries))
}

#[derive(Debug, Deserialize)]
struct TrainingLexiconEntry {
    text: String,
    reading: String,
    #[serde(default = "default_weight")]
    weight: f32,
}

fn default_weight() -> f32 {
    1.0
}

#[derive(Debug, Deserialize)]
struct ArtifactRegistry {
    #[serde(default)]
    language_packs: Vec<RegisteredLanguagePack>,
    #[serde(default)]
    observation_models: Vec<RegisteredObservationModel>,
}

#[derive(Debug, Deserialize)]
struct RegisteredLanguagePack {
    language_pack: PathBuf,
    status: String,
    #[serde(default)]
    has_pack: bool,
}

#[derive(Debug, Deserialize)]
struct RegisteredObservationModel {
    path: PathBuf,
    status: String,
}
