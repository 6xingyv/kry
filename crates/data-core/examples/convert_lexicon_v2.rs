// One-time: read a lexicon .bin (legacy MOCHLX01 or already-v2) and rewrite it as
// the mmap-friendly MOCHLX02 image.
use data_core::{read_lexicon_component, write_lexicon_bin};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    for path in std::env::args().skip(1) {
        let lex = read_lexicon_component(&path)?;
        write_lexicon_bin(&path, "", &lex)?;
        println!("converted {path}: {} entries", lex.entry_count());
    }
    Ok(())
}
