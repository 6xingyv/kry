use std::error::Error;
use std::fs::File;
use std::path::PathBuf;

use data_core::{LanguagePack, ObservationModelPack};
use serde::Deserialize;

fn main() -> Result<(), Box<dyn Error>> {
    let registry_path = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("assets/artifact-registry.json"));
    let registry: ArtifactRegistry = serde_json::from_reader(File::open(&registry_path)?)?;

    println!(
        "registry={} format={}",
        registry_path.display(),
        registry.format
    );
    if let Some(note) = registry.note.as_deref() {
        println!("note={note}");
    }

    for entry in registry.language_packs {
        if entry.status != "ready" || !entry.has_pack {
            println!(
                "language_pack schema={} status={} skipped reason={}",
                entry.schema, entry.status, entry.reason
            );
            continue;
        }

        let pack = LanguagePack::load(&entry.language_pack)?;
        for component in &entry.required_components {
            if !pack.manifest.components.contains_key(component) {
                return Err(format!(
                    "pack {} is missing required component {}",
                    entry.schema, component
                )
                .into());
            }
        }

        let syllables = pack
            .schema_fst
            .as_ref()
            .map(|fst| fst.syllables.len())
            .unwrap_or_default();
        let frequency_entries = pack
            .frequency
            .as_ref()
            .map(|frequency| frequency.entries)
            .unwrap_or_default();
        let aliases = pack
            .alias_table
            .as_ref()
            .map(|table| table.aliases.len())
            .unwrap_or_default();
        let context_continuations = pack
            .context_model
            .as_ref()
            .map(|model| model.continuations.len())
            .unwrap_or_default();
        let morphology_entries = pack
            .morphology
            .as_ref()
            .map(|table| table.entries)
            .unwrap_or_default();
        let transliteration_rules = pack
            .transliteration_table
            .as_ref()
            .map(|table| table.rules.len())
            .unwrap_or_default();
        println!(
            "language_pack schema={} entries={} readings={} syllables={} aliases={} context_continuations={} frequency_entries={} morphology_entries={} transliteration_rules={} root={}",
            pack.manifest.schema,
            pack.lexicon.entry_count(),
            pack.lexicon.reading_count(),
            syllables,
            aliases,
            context_continuations,
            frequency_entries,
            morphology_entries,
            transliteration_rules,
            pack.root.display()
        );
    }

    for entry in registry.observation_models {
        if entry.status != "ready" {
            println!(
                "observation_model name={} status={} skipped",
                entry.name, entry.status
            );
            continue;
        }

        let pack = ObservationModelPack::load(&entry.path)?;
        let (gesture_templates, gesture_points) = pack
            .gesture_templates
            .as_ref()
            .map(|templates| (templates.templates.len(), templates.point_count))
            .unwrap_or_default();
        println!(
            "observation_model name={} geometry={} keymap={} samples={} slot_errors={} gesture_templates={} gesture_points={} root={}",
            entry.name,
            pack.error_model.geometry,
            pack.error_model.keymap_reference,
            pack.error_model.samples,
            pack.error_model.slot_errors.len(),
            gesture_templates,
            gesture_points,
            pack.root.display()
        );
    }

    Ok(())
}

#[derive(Debug, Deserialize)]
struct ArtifactRegistry {
    format: String,
    #[serde(default)]
    note: Option<String>,
    #[serde(default)]
    language_packs: Vec<RegisteredLanguagePack>,
    #[serde(default)]
    observation_models: Vec<RegisteredObservationModel>,
}

#[derive(Debug, Deserialize)]
struct RegisteredLanguagePack {
    schema: String,
    language_pack: PathBuf,
    #[serde(default)]
    required_components: Vec<String>,
    status: String,
    #[serde(default)]
    reason: String,
    #[serde(default)]
    has_pack: bool,
}

#[derive(Debug, Deserialize)]
struct RegisteredObservationModel {
    name: String,
    path: PathBuf,
    status: String,
}
