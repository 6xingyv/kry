use std::cell::OnceCell;
use std::collections::HashMap;
use std::io::{self, BufRead};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

pub type SyllableId = u32;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SyllableTable {
    ids_by_text: HashMap<String, SyllableId>,
    text_by_id: Vec<String>,
}

impl SyllableTable {
    pub fn intern(&mut self, syllable: &str) -> SyllableId {
        if let Some(id) = self.ids_by_text.get(syllable) {
            return *id;
        }
        let id = self.text_by_id.len() as SyllableId;
        self.ids_by_text.insert(syllable.to_owned(), id);
        self.text_by_id.push(syllable.to_owned());
        id
    }

    pub fn get(&self, id: SyllableId) -> Option<&str> {
        self.text_by_id.get(id as usize).map(String::as_str)
    }

    pub fn id(&self, syllable: &str) -> Option<SyllableId> {
        self.ids_by_text.get(syllable).copied()
    }

    pub fn len(&self) -> usize {
        self.text_by_id.len()
    }

    pub fn is_empty(&self) -> bool {
        self.text_by_id.is_empty()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ReadingKey(pub Vec<SyllableId>);

/// An owned lexicon entry — used when *building* a lexicon and as the return type
/// of lookups (copied out of the backing store).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LexiconEntry {
    pub text: String,
    pub reading: String,
    pub weight: f32,
}

impl LexiconEntry {
    pub fn new(text: impl Into<String>, reading: impl Into<String>, weight: f32) -> Self {
        Self {
            text: text.into(),
            reading: normalize_reading(&reading.into()),
            weight,
        }
    }
}

// ── On-disk / in-memory binary format (MOCHLX02) ───────────────────────────────
//
// librime-style: the lexicon is a flat, position-independent byte image that is
// memory-mapped (or read once into a Vec) and queried IN PLACE — no entries, maps,
// or tries are materialized at load. Records are sorted by (reading asc, weight
// desc), so exact lookup is a binary search over a contiguous, already-ranked range
// and prefix lookup is a forward scan. Strings live in a shared blob, referenced by
// (offset, len). Only a tiny header is parsed at construction; load is O(1).
//
//   [0..8]   magic "MOCHLX02"
//   [8..12]  record_count : u32
//   [12..16] blob_len     : u32
//   [16..20] max_weight   : f32
//   [20..]   records      : record_count × Record(20B) {text_off,text_len,reading_off,reading_len,weight}
//   [..]     by_weight    : record_count × u32  (record indices, weight desc)
//   [..]     blob         : blob_len bytes (UTF-8 strings)

const MAGIC_V2: &[u8; 8] = b"MOCHLX02";
const MAGIC_V1: &[u8; 8] = b"MOCHLX01";
const RECORD_SIZE: usize = 20;
const HEADER_SIZE: usize = 20;

#[derive(Debug)]
enum Backing {
    Mmap(memmap2::Mmap),
    Owned(Vec<u8>),
}

impl Backing {
    #[inline]
    fn bytes(&self) -> &[u8] {
        match self {
            Backing::Mmap(m) => &m[..],
            Backing::Owned(v) => &v[..],
        }
    }
}

#[derive(Clone)]
pub struct Lexicon {
    backing: Arc<Backing>,
    record_count: usize,
    records_off: usize,
    by_weight_off: usize,
    blob_off: usize,
    max_weight: f32,
    // Built on demand the first time syllable APIs are used (tests/tools only;
    // never touched on the decode hot path), so it costs nothing at load.
    syllables: OnceCell<SyllableTable>,
}

impl std::fmt::Debug for Lexicon {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Lexicon")
            .field("entries", &self.record_count)
            .field("max_weight", &self.max_weight)
            .finish()
    }
}

impl Default for Lexicon {
    fn default() -> Self {
        Self::new([])
    }
}

#[inline]
fn read_u32(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
}

#[inline]
fn read_f32(bytes: &[u8], at: usize) -> f32 {
    f32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
}

impl Lexicon {
    pub fn new(entries: impl IntoIterator<Item = LexiconEntry>) -> Self {
        let bytes = serialize_lexicon(entries);
        Self::from_backing(Backing::Owned(bytes))
            .expect("freshly serialized lexicon must parse")
    }

    /// Memory-map a `.bin` lexicon. MOCHLX02 maps zero-copy; a legacy MOCHLX01 file
    /// is read and re-serialized in memory (still correct, just not mmap-backed) so
    /// existing assets keep working until they are reconverted.
    pub fn open_mmap(path: &std::path::Path) -> io::Result<Self> {
        let file = std::fs::File::open(path)?;
        // Safety: the asset is read-only and not mutated while mapped.
        let mmap = unsafe { memmap2::Mmap::map(&file)? };
        if mmap.len() >= 8 && &mmap[0..8] == MAGIC_V2 {
            Self::from_backing(Backing::Mmap(mmap))
                .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "corrupt MOCHLX02"))
        } else if mmap.len() >= 8 && &mmap[0..8] == MAGIC_V1 {
            let entries = read_flat_lexicon_v1(&mmap)?;
            Ok(Self::new(entries))
        } else {
            Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "not a mocha lexicon artifact",
            ))
        }
    }

    fn from_backing(backing: Backing) -> Option<Self> {
        let bytes = backing.bytes();
        if bytes.len() < HEADER_SIZE || &bytes[0..8] != MAGIC_V2 {
            return None;
        }
        let record_count = read_u32(bytes, 8) as usize;
        let blob_len = read_u32(bytes, 12) as usize;
        let max_weight = read_f32(bytes, 16);
        let records_off = HEADER_SIZE;
        let by_weight_off = records_off + record_count * RECORD_SIZE;
        let blob_off = by_weight_off + record_count * 4;
        if blob_off + blob_len > bytes.len() {
            return None;
        }
        Some(Self {
            backing: Arc::new(backing),
            record_count,
            records_off,
            by_weight_off,
            blob_off,
            max_weight,
            syllables: OnceCell::new(),
        })
    }

    #[inline]
    fn bytes(&self) -> &[u8] {
        self.backing.bytes()
    }

    #[inline]
    fn str_at(&self, off: u32, len: u32) -> &str {
        let bytes = self.bytes();
        let start = self.blob_off + off as usize;
        std::str::from_utf8(&bytes[start..start + len as usize]).unwrap_or("")
    }

    #[inline]
    fn reading_at(&self, record: usize) -> &str {
        let o = self.records_off + record * RECORD_SIZE;
        let bytes = self.bytes();
        self.str_at(read_u32(bytes, o + 8), read_u32(bytes, o + 12))
    }

    fn entry_at(&self, record: usize) -> LexiconEntry {
        let o = self.records_off + record * RECORD_SIZE;
        let bytes = self.bytes();
        LexiconEntry {
            text: self
                .str_at(read_u32(bytes, o), read_u32(bytes, o + 4))
                .to_owned(),
            reading: self
                .str_at(read_u32(bytes, o + 8), read_u32(bytes, o + 12))
                .to_owned(),
            weight: read_f32(bytes, o + 16),
        }
    }

    /// First record index whose reading is `>= target` (records sorted by reading).
    fn lower_bound(&self, target: &str) -> usize {
        let (mut lo, mut hi) = (0usize, self.record_count);
        while lo < hi {
            let mid = (lo + hi) / 2;
            if self.reading_at(mid) < target {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        lo
    }

    pub fn lookup_reading(&self, reading: &str) -> Vec<LexiconEntry> {
        let normalized = normalize_reading(reading);
        let mut out = Vec::new();
        let mut i = self.lower_bound(&normalized);
        while i < self.record_count && self.reading_at(i) == normalized {
            out.push(self.entry_at(i)); // already weight-desc within a reading
            i += 1;
        }
        out
    }

    pub fn lookup_reading_prefix(&self, reading_prefix: &str, limit: usize) -> Vec<LexiconEntry> {
        if limit == 0 {
            return Vec::new();
        }
        let normalized = normalize_reading(reading_prefix);
        if normalized.is_empty() {
            return Vec::new();
        }
        let with_space = format!("{normalized} ");
        let mut matched = Vec::new();
        let mut i = self.lower_bound(&normalized);
        // A syllable-prefix match is either the exact reading or one starting with
        // "<prefix> ". Both sort contiguously right after the lower bound (the space
        // 0x20 sorts before any syllable letter), so we can stop at the first miss.
        while i < self.record_count {
            let reading = self.reading_at(i);
            if reading == normalized || reading.starts_with(&with_space) {
                matched.push(self.entry_at(i));
                i += 1;
            } else {
                break;
            }
        }
        matched.sort_by(|a, b| b.weight.total_cmp(&a.weight));
        matched.truncate(limit);
        matched
    }

    pub fn has_reading_prefix(&self, reading_prefix: &str) -> bool {
        let normalized = normalize_reading(reading_prefix);
        if normalized.is_empty() {
            return false;
        }
        let with_space = format!("{normalized} ");
        let i = self.lower_bound(&normalized);
        if i >= self.record_count {
            return false;
        }
        let reading = self.reading_at(i);
        reading == normalized || reading.starts_with(&with_space)
    }

    pub fn entry_count(&self) -> usize {
        self.record_count
    }

    pub fn reading_count(&self) -> usize {
        let mut count = 0usize;
        let mut prev: Option<&str> = None;
        for i in 0..self.record_count {
            let reading = self.reading_at(i);
            if prev != Some(reading) {
                count += 1;
                prev = Some(reading);
            }
        }
        count
    }

    pub fn max_weight(&self) -> f32 {
        self.max_weight
    }

    /// Entry at frequency rank `idx` (0 = highest weight). Lets callers that index
    /// the old `entries()` slice keep working (e.g. the English fuzzy index).
    pub fn entry(&self, idx: usize) -> Option<LexiconEntry> {
        if idx >= self.record_count {
            return None;
        }
        let record = read_u32(self.bytes(), self.by_weight_off + idx * 4) as usize;
        Some(self.entry_at(record))
    }

    /// Iterates all entries in descending-weight order.
    pub fn iter_entries(&self) -> impl Iterator<Item = LexiconEntry> + '_ {
        (0..self.record_count).filter_map(move |idx| self.entry(idx))
    }

    pub fn entry_cost(&self, entry: &LexiconEntry) -> f32 {
        if self.max_weight <= 0.0 || entry.weight <= 0.0 {
            return 12.0;
        }
        ((self.max_weight + 1.0) / (entry.weight + 1.0)).ln()
    }

    fn syllable_table(&self) -> &SyllableTable {
        self.syllables.get_or_init(|| {
            let mut table = SyllableTable::default();
            for i in 0..self.record_count {
                for syllable in self.reading_at(i).split_whitespace() {
                    table.intern(syllable);
                }
            }
            table
        })
    }

    pub fn syllables(&self) -> &SyllableTable {
        self.syllable_table()
    }

    pub fn compressed_key(&self, reading: &str) -> Option<ReadingKey> {
        let table = self.syllable_table();
        normalize_reading(reading)
            .split_whitespace()
            .map(|syllable| table.id(syllable))
            .collect::<Option<Vec<_>>>()
            .map(ReadingKey)
    }

    pub fn decode_key(&self, key: ReadingKey) -> Option<String> {
        let table = self.syllable_table();
        key.0
            .iter()
            .map(|id| table.get(*id))
            .collect::<Option<Vec<_>>>()
            .map(|parts| parts.join(" "))
    }

    pub fn read_english_word_list(reader: impl BufRead, limit: usize) -> io::Result<Self> {
        let mut entries = Vec::new();
        for (idx, line) in reader.lines().enumerate() {
            if idx >= limit {
                break;
            }
            let word = line?.trim().to_ascii_lowercase();
            if word.is_empty() {
                continue;
            }
            entries.push(LexiconEntry::new(
                word.clone(),
                word,
                (limit.saturating_sub(idx)) as f32,
            ));
        }
        Ok(Self::new(entries))
    }

    pub fn read_rime_dict(reader: impl BufRead, limit: usize) -> io::Result<Self> {
        let mut in_body = false;
        let mut entries = Vec::new();
        for line in reader.lines() {
            let line = line?;
            let trimmed = line.trim();
            if trimmed == "..." {
                in_body = true;
                continue;
            }
            if !in_body || trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            let mut fields = trimmed.split('\t');
            let Some(text) = fields.next() else { continue };
            let Some(reading) = fields.next() else {
                continue;
            };
            let weight = fields
                .next()
                .and_then(|raw| raw.parse::<f32>().ok())
                .unwrap_or(1.0);
            entries.push(LexiconEntry::new(text, reading, weight));
            if entries.len() >= limit {
                break;
            }
        }
        Ok(Self::new(entries))
    }
}

/// Serializes entries into the MOCHLX02 byte image (records sorted by reading asc,
/// then weight desc; strings deduplicated into a shared blob).
pub fn serialize_lexicon(entries: impl IntoIterator<Item = LexiconEntry>) -> Vec<u8> {
    let mut entries: Vec<LexiconEntry> = entries.into_iter().collect();
    entries.sort_by(|a, b| {
        a.reading
            .cmp(&b.reading)
            .then_with(|| b.weight.total_cmp(&a.weight))
            .then_with(|| a.text.cmp(&b.text))
    });

    let mut blob = Vec::<u8>::new();
    let mut interned: HashMap<String, (u32, u32)> = HashMap::new();
    let mut intern = |blob: &mut Vec<u8>, s: &str| -> (u32, u32) {
        if let Some(&pos) = interned.get(s) {
            return pos;
        }
        let pos = (blob.len() as u32, s.len() as u32);
        blob.extend_from_slice(s.as_bytes());
        interned.insert(s.to_owned(), pos);
        pos
    };

    let mut records: Vec<[u8; RECORD_SIZE]> = Vec::with_capacity(entries.len());
    let mut max_weight = 0.0f32;
    for entry in &entries {
        let (text_off, text_len) = intern(&mut blob, &entry.text);
        let (reading_off, reading_len) = intern(&mut blob, &entry.reading);
        max_weight = max_weight.max(entry.weight);
        let mut rec = [0u8; RECORD_SIZE];
        rec[0..4].copy_from_slice(&text_off.to_le_bytes());
        rec[4..8].copy_from_slice(&text_len.to_le_bytes());
        rec[8..12].copy_from_slice(&reading_off.to_le_bytes());
        rec[12..16].copy_from_slice(&reading_len.to_le_bytes());
        rec[16..20].copy_from_slice(&entry.weight.to_le_bytes());
        records.push(rec);
    }

    // by_weight: record indices sorted by weight desc (frequency rank order).
    let mut by_weight: Vec<u32> = (0..records.len() as u32).collect();
    by_weight.sort_by(|&a, &b| {
        entries[b as usize]
            .weight
            .total_cmp(&entries[a as usize].weight)
            .then_with(|| entries[a as usize].text.cmp(&entries[b as usize].text))
    });

    let mut out = Vec::with_capacity(
        HEADER_SIZE + records.len() * RECORD_SIZE + by_weight.len() * 4 + blob.len(),
    );
    out.extend_from_slice(MAGIC_V2);
    out.extend_from_slice(&(records.len() as u32).to_le_bytes());
    out.extend_from_slice(&(blob.len() as u32).to_le_bytes());
    out.extend_from_slice(&max_weight.to_le_bytes());
    for rec in &records {
        out.extend_from_slice(rec);
    }
    for idx in &by_weight {
        out.extend_from_slice(&idx.to_le_bytes());
    }
    out.extend_from_slice(&blob);
    out
}

/// Reads a legacy MOCHLX01 image into owned entries (for backward compatibility and
/// one-time conversion to MOCHLX02).
fn read_flat_lexicon_v1(bytes: &[u8]) -> io::Result<Vec<LexiconEntry>> {
    let err = || io::Error::new(io::ErrorKind::InvalidData, "corrupt MOCHLX01");
    if bytes.len() < 20 || &bytes[0..8] != MAGIC_V1 {
        return Err(err());
    }
    let schema_len = read_u32(bytes, 8) as usize;
    let entry_count = read_u32(bytes, 12) as usize;
    let blob_len = read_u32(bytes, 16) as usize;
    let mut cursor = 20 + schema_len;
    let records_start = cursor;
    let blob_start = records_start + entry_count * 20;
    if blob_start + blob_len > bytes.len() {
        return Err(err());
    }
    let blob = &bytes[blob_start..blob_start + blob_len];
    let read_str = |off: u32, len: u32| -> io::Result<String> {
        let (o, l) = (off as usize, len as usize);
        if o + l > blob.len() {
            return Err(err());
        }
        std::str::from_utf8(&blob[o..o + l])
            .map(str::to_owned)
            .map_err(|_| err())
    };
    let mut entries = Vec::with_capacity(entry_count);
    for _ in 0..entry_count {
        let text_off = read_u32(bytes, cursor);
        let text_len = read_u32(bytes, cursor + 4);
        let reading_off = read_u32(bytes, cursor + 8);
        let reading_len = read_u32(bytes, cursor + 12);
        let weight = read_f32(bytes, cursor + 16);
        cursor += 20;
        entries.push(LexiconEntry {
            text: read_str(text_off, text_len)?,
            reading: read_str(reading_off, reading_len)?,
            weight,
        });
    }
    Ok(entries)
}

pub fn compress_reading(reading: &str) -> (SyllableTable, ReadingKey) {
    let mut table = SyllableTable::default();
    let key = ReadingKey(
        normalize_reading(reading)
            .split_whitespace()
            .map(|syllable| table.intern(syllable))
            .collect(),
    );
    (table, key)
}

pub fn normalize_reading(reading: &str) -> String {
    reading
        .trim()
        .split_whitespace()
        .map(|part| part.chars().map(fold_pinyin_char).collect::<String>())
        .collect::<Vec<_>>()
        .join(" ")
}

fn fold_pinyin_char(ch: char) -> char {
    match ch {
        'ā' | 'á' | 'ǎ' | 'à' | 'Ā' | 'Á' | 'Ǎ' | 'À' => 'a',
        'ē' | 'é' | 'ě' | 'è' | 'Ē' | 'É' | 'Ě' | 'È' => 'e',
        'ī' | 'í' | 'ǐ' | 'ì' | 'Ī' | 'Í' | 'Ǐ' | 'Ì' => 'i',
        'ō' | 'ó' | 'ǒ' | 'ò' | 'Ō' | 'Ó' | 'Ǒ' | 'Ò' => 'o',
        'ū' | 'ú' | 'ǔ' | 'ù' | 'Ū' | 'Ú' | 'Ǔ' | 'Ù' => 'u',
        'ǖ' | 'ǘ' | 'ǚ' | 'ǜ' | 'Ü' | 'ü' => 'v',
        _ => ch.to_ascii_lowercase(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_rime_dict_body() {
        let input = b"# header\n---\n...\n\xE8\xA5\xBF\xE5\xAE\x89\txi an\t100\n";
        let lexicon = Lexicon::read_rime_dict(&input[..], 10).unwrap();
        assert_eq!(lexicon.lookup_reading("xi an")[0].text, "西安");
        assert_eq!(lexicon.syllables().len(), 2);
        assert_eq!(
            lexicon
                .decode_key(lexicon.compressed_key("xi an").unwrap())
                .unwrap(),
            "xi an"
        );
    }

    #[test]
    fn folds_tones() {
        assert_eq!(normalize_reading("nǐ hǎo"), "ni hao");
    }

    #[test]
    fn reading_prefix_returns_weighted_matches() {
        let lexicon = Lexicon::new([
            LexiconEntry::new("一个人", "yi ge ren", 10.0),
            LexiconEntry::new("一个", "yi ge", 100.0),
            LexiconEntry::new("一", "yi", 50.0),
            LexiconEntry::new("以前", "yi qian", 40.0),
        ]);
        let matches = lexicon.lookup_reading_prefix("yi ge", 3);
        assert_eq!(matches[0].text, "一个");
        assert_eq!(matches[1].text, "一个人");
        assert!(lexicon.has_reading_prefix("yi"));
        assert!(!lexicon.has_reading_prefix("bu"));
    }

    #[test]
    fn reading_prefix_without_exact_reading() {
        let lexicon = Lexicon::new([
            LexiconEntry::new("西安", "xi an", 100.0),
            LexiconEntry::new("喜欢", "xi huan", 90.0),
        ]);
        assert!(lexicon.has_reading_prefix("xi"));
        let matches = lexicon.lookup_reading_prefix("xi", 4);
        assert_eq!(matches[0].text, "西安");
        assert_eq!(matches[1].text, "喜欢");
    }

    #[test]
    fn lookup_reading_is_weight_sorted() {
        let lexicon = Lexicon::new([
            LexiconEntry::new("见", "jian", 50.0),
            LexiconEntry::new("件", "jian", 90.0),
            LexiconEntry::new("间", "jian", 70.0),
        ]);
        let m = lexicon.lookup_reading("jian");
        assert_eq!(m.iter().map(|e| e.text.as_str()).collect::<Vec<_>>(), ["件", "间", "见"]);
    }
}
