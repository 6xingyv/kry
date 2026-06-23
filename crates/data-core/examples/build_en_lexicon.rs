// Build an en-word lexicon.bin from a `word<TAB>count` frequency list.
// Usage: cargo run -p data-core --example build_en_lexicon -- <in.tsv> <out.bin>
use std::fs;
use std::io::{BufRead, BufReader};

use data_core::{LexiconEntry, serialize_lexicon};

fn main() {
    let in_path = std::env::args().nth(1).expect("usage: <in.tsv> <out.bin>");
    let out_path = std::env::args().nth(2).expect("usage: <in.tsv> <out.bin>");

    let file = fs::File::open(&in_path).expect("open input");
    let mut entries = Vec::new();
    for line in BufReader::new(file).lines() {
        let line = line.expect("read line");
        let mut cols = line.split('\t');
        let (Some(word), Some(count)) = (cols.next(), cols.next()) else {
            continue;
        };
        let word = word.trim();
        let count: f32 = count.trim().parse().unwrap_or(0.0);
        if word.is_empty() || count <= 0.0 {
            continue;
        }
        // Log-compress the count into the weight. Raw counts give entry_cost too
        // large a spread (a higher-freq neighbor like "you" overrides the geometry
        // of "your"); log keeps frequency a gentle prior so observation decides
        // among plausible words, while junk still sorts to the bottom.
        // Scaled ×1000 so the f32 weight keeps resolution.
        let weight = (1.0 + count).ln() * 1000.0;
        // English: text == reading == the (swipeable) lowercase word.
        entries.push(LexiconEntry::new(word, word, weight));
    }

    let n = entries.len();
    let bytes = serialize_lexicon(entries);
    fs::write(&out_path, &bytes).expect("write output");
    eprintln!("wrote {n} entries ({} bytes) to {out_path}", bytes.len());
}
