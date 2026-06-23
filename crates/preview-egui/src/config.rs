use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct PreviewConfig {
    pub language_pack_root: PathBuf,
    pub observation_pack_root: PathBuf,
}

impl PreviewConfig {
    pub fn parse() -> Self {
        let mut language_pack_root = PathBuf::from("assets/language-packs");
        let mut observation_pack_root =
            PathBuf::from("assets/observation-models/geometry-phone-10col/qwerty");
        let mut args = std::env::args().skip(1);

        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--language-packs" => {
                    if let Some(value) = args.next() {
                        language_pack_root = PathBuf::from(value);
                    }
                }
                "--observation-pack" => {
                    if let Some(value) = args.next() {
                        observation_pack_root = PathBuf::from(value);
                    }
                }
                "--help" | "-h" => {
                    eprintln!(
                        "usage: preview-egui --language-packs assets/language-packs --observation-pack assets/observation-models/geometry-phone-10col/qwerty"
                    );
                    std::process::exit(0);
                }
                _ => {}
            }
        }

        Self {
            language_pack_root,
            observation_pack_root,
        }
    }
}
