use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use data_core::LanguagePack;
use engine_core::ImeEngine;
use geometry_core::{GeometryLayout, Point};
use geometry_phone_10col::Phone10ColGeometry;
use keymap_latin_qwerty::LatinQwertyKeyMap;

struct TestWord {
    text: String,
    reading: String,
    symbol_path: String,
}

fn main() -> Result<(), Box<dyn Error>> {
    let pack_root = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("assets/language-packs"));
    let observation_root = std::env::args()
        .nth(2)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("assets/observation-models/geometry-phone-10col/qwerty"));
    let limit = std::env::args()
        .nth(3)
        .and_then(|v| v.parse::<usize>().ok());
    let corpus_path = std::env::args()
        .nth(4)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("datasets/corpus/zh-wiki.txt"));

    let engine = ImeEngine::zh_qwerty_from_artifacts(&pack_root, &observation_root)?;
    let geometry = Phone10ColGeometry::new();
    let keymap = LatinQwertyKeyMap::new();

    // Per-(word, reading) frequency, so accuracy can be judged the way users feel
    // it: a miss on 的/我们 counts far more than a miss on a rare word.
    let zh_pack = LanguagePack::load(pack_root.join("zh-hans-pinyin-full"))?;
    let mut weights: HashMap<(String, String), f64> = HashMap::new();
    for e in zh_pack.lexicon.iter_entries() {
        let w = weights
            .entry((e.text.clone(), e.reading.clone()))
            .or_insert(0.0);
        *w = (*w).max(e.weight as f64);
    }

    // ── Part 1: 真实文章测试 ──
    println!("═══ 真实文章测试 ═══");
    let article_words = article_test_data();
    run_benchmark(&engine, &geometry, &keymap, &article_words, "article", &weights);

    // ── Part 2: 词库频率测试 ──
    println!("\n═══ 词库频率测试 ═══");
    let lexicon_words = lexicon_test_data(&zh_pack, limit.unwrap_or(5000));
    println!(
        "test_words: {} entries (1-3 syllables, sorted by frequency)",
        lexicon_words.len()
    );
    run_benchmark(&engine, &geometry, &keymap, &lexicon_words, "lexicon", &weights);

    // ── Part 3: 语料高频 3-4 字长拼音切分测试 ──
    println!("\n═══ 语料 3-4 字长拼音测试 ═══");
    let corpus_chunks =
        corpus_chunk_pinyin_test_data(&zh_pack, &corpus_path, limit.unwrap_or(200), 9)?;
    let chunk_avg_symbols = if corpus_chunks.is_empty() {
        0.0
    } else {
        corpus_chunks
            .iter()
            .map(|word| word.symbol_path.len())
            .sum::<usize>() as f64
            / corpus_chunks.len() as f64
    };
    println!(
        "test_words: {} corpus chunks (3-4 CJK chars from {}, avg compact pinyin len {:.1})",
        corpus_chunks.len(),
        corpus_path.display(),
        chunk_avg_symbols,
    );
    run_benchmark(
        &engine,
        &geometry,
        &keymap,
        &corpus_chunks,
        "corpus_chunk_3_4",
        &weights,
    );

    let long_words = corpus_long_pinyin_test_data(&zh_pack, &corpus_path, limit.unwrap_or(200), 9)?;
    let avg_symbols = if long_words.is_empty() {
        0.0
    } else {
        long_words
            .iter()
            .map(|word| word.symbol_path.len())
            .sum::<usize>() as f64
            / long_words.len() as f64
    };
    println!(
        "test_words: {} entries (3-4 CJK chars/syllables from {}, avg compact pinyin len {:.1})",
        long_words.len(),
        corpus_path.display(),
        avg_symbols,
    );
    run_benchmark(&engine, &geometry, &keymap, &long_words, "corpus_3_4", &weights);

    let very_long_words =
        corpus_long_pinyin_test_data(&zh_pack, &corpus_path, limit.unwrap_or(200), 13)?;
    let very_long_avg_symbols = if very_long_words.is_empty() {
        0.0
    } else {
        very_long_words
            .iter()
            .map(|word| word.symbol_path.len())
            .sum::<usize>() as f64
            / very_long_words.len() as f64
    };
    println!(
        "\ntest_words: {} entries (3-4 CJK chars/syllables, compact pinyin len >=13, avg {:.1})",
        very_long_words.len(),
        very_long_avg_symbols,
    );
    run_benchmark(
        &engine,
        &geometry,
        &keymap,
        &very_long_words,
        "corpus_3_4_long",
        &weights,
    );

    Ok(())
}

fn run_benchmark(
    engine: &ImeEngine,
    geometry: &Phone10ColGeometry,
    keymap: &LatinQwertyKeyMap,
    words: &[TestWord],
    tag: &str,
    weights: &HashMap<(String, String), f64>,
) {
    let noise_levels: &[(&str, f32)] = &[
        ("no_noise", 0.0),
        ("mild", 0.015),
        ("moderate", 0.03),
        ("heavy", 0.05),
    ];

    for &(label, noise) in noise_levels {
        let (stats, mismatches) = evaluate(engine, geometry, keymap, words, noise, weights);
        let n = stats.total as f64;
        let wt = stats.w_total.max(1e-9);
        println!(
            "{tag}_{label:>8}: n={:<5} | freq-wt reading_top1={:.1}% text_top1={:.1}% | unwt reading={:.1}% text={:.1}% top5(r)={:.1}% avg={:.2}ms",
            stats.total,
            stats.w_reading_top1 / wt * 100.0,
            stats.w_text_top1 / wt * 100.0,
            stats.reading_top1 as f64 / n * 100.0,
            stats.text_top1 as f64 / n * 100.0,
            stats.reading_top5 as f64 / n * 100.0,
            stats.total_time.as_secs_f64() / n * 1000.0,
        );
        if label == "mild" && !mismatches.is_empty() {
            for m in mismatches.iter().take(10) {
                println!("  {m}");
            }
        }
    }
}

struct Stats {
    total: usize,
    reading_top1: usize,
    text_top1: usize,
    reading_top5: usize,
    text_top5: usize,
    total_time: Duration,
    // Frequency-weighted accumulators: Σ(freq·correct) / Σ(freq).
    w_total: f64,
    w_reading_top1: f64,
    w_text_top1: f64,
}

fn evaluate(
    engine: &ImeEngine,
    geometry: &Phone10ColGeometry,
    keymap: &LatinQwertyKeyMap,
    words: &[TestWord],
    noise: f32,
    weights: &HashMap<(String, String), f64>,
) -> (Stats, Vec<String>) {
    let mut stats = Stats {
        total: 0,
        reading_top1: 0,
        text_top1: 0,
        reading_top5: 0,
        text_top5: 0,
        total_time: Duration::ZERO,
        w_total: 0.0,
        w_reading_top1: 0.0,
        w_text_top1: 0.0,
    };
    let mut mismatches = Vec::new();

    for (idx, w) in words.iter().enumerate() {
        let Some(trace) = generate_trace(&w.symbol_path, geometry, keymap, noise, idx as u64)
        else {
            continue;
        };

        let start = Instant::now();
        let candidates = engine.decode_gesture_trace(&trace, 8);
        stats.total_time += start.elapsed();
        stats.total += 1;
        let weight = weights
            .get(&(w.text.clone(), w.reading.clone()))
            .copied()
            .unwrap_or(1.0)
            .max(1.0);
        stats.w_total += weight;

        let top = candidates.top();
        if top.is_some_and(|c| c.reading == w.reading) {
            stats.reading_top1 += 1;
            stats.w_reading_top1 += weight;
        }
        if top.is_some_and(|c| c.text == w.text) {
            stats.text_top1 += 1;
            stats.w_text_top1 += weight;
        }
        if candidates
            .candidates
            .iter()
            .take(5)
            .any(|c| c.reading == w.reading)
        {
            stats.reading_top5 += 1;
        }
        if candidates
            .candidates
            .iter()
            .take(5)
            .any(|c| c.text == w.text)
        {
            stats.text_top5 += 1;
        }
        if !top.is_some_and(|c| c.reading == w.reading) && mismatches.len() < 15 {
            let got = top
                .map(|c| format!("{}({})", c.text, c.reading))
                .unwrap_or_else(|| "<none>".to_owned());
            mismatches.push(format!("expect {}({}) → got {}", w.text, w.reading, got));
        }
    }

    (stats, mismatches)
}

fn generate_trace(
    symbol_path: &str,
    geometry: &Phone10ColGeometry,
    keymap: &LatinQwertyKeyMap,
    noise_scale: f32,
    seed: u64,
) -> Option<Vec<Point>> {
    let centers: Vec<Point> = symbol_path
        .chars()
        .map(|ch| {
            let slot_id = keymap.slot_for_symbol(ch)?;
            Some(geometry.slot(&slot_id)?.center())
        })
        .collect::<Option<Vec<_>>>()?;

    if centers.len() < 2 {
        return None;
    }

    let points_per_segment = 8;
    let mut trace = vec![centers[0]];
    for i in 1..centers.len() {
        let from = centers[i - 1];
        let to = centers[i];
        for j in 1..=points_per_segment {
            let t = j as f32 / points_per_segment as f32;
            trace.push(Point::new(
                from.x + (to.x - from.x) * t,
                from.y + (to.y - from.y) * t,
            ));
        }
    }

    if noise_scale > 0.0 {
        let mut state = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        for point in &mut trace {
            state = state
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            let nx = ((state >> 33) as f32 / u32::MAX as f32 - 0.5) * 2.0 * noise_scale;
            state = state
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            let ny = ((state >> 33) as f32 / u32::MAX as f32 - 0.5) * 2.0 * noise_scale;
            point.x += nx;
            point.y += ny;
        }
    }

    Some(trace)
}

fn article_test_data() -> Vec<TestWord> {
    // 真实文章：《移动互联网时代的输入法》
    let raw: &[(&str, &str)] = &[
        // 第一段
        ("随着", "sui zhe"),
        ("移动", "yi dong"),
        ("互联网", "hu lian wang"),
        ("的", "de"),
        ("发展", "fa zhan"),
        ("智能", "zhi neng"),
        ("手机", "shou ji"),
        ("已经", "yi jing"),
        ("成为", "cheng wei"),
        ("人们", "ren men"),
        ("生活", "sheng huo"),
        ("中", "zhong"),
        ("不可", "bu ke"),
        ("缺少", "que shao"),
        ("一部分", "yi bu fen"),
        // 第二段
        ("输入法", "shu ru fa"),
        ("作为", "zuo wei"),
        ("人机", "ren ji"),
        ("交互", "jiao hu"),
        ("最", "zui"),
        ("基础", "ji chu"),
        ("工具", "gong ju"),
        ("其", "qi"),
        ("重要性", "zhong yao xing"),
        ("不言", "bu yan"),
        ("而喻", "er yu"),
        // 第三段
        ("中文", "zhong wen"),
        ("输入", "shu ru"),
        ("面临", "mian lin"),
        ("独特", "du te"),
        ("挑战", "tiao zhan"),
        ("拼音", "pin yin"),
        ("需要", "xu yao"),
        ("准确", "zhun que"),
        ("音节", "yin jie"),
        ("切分", "qie fen"),
        ("同时", "tong shi"),
        ("还要", "hai yao"),
        ("处理", "chu li"),
        ("多音字", "duo yin zi"),
        ("问题", "wen ti"),
        // 第四段
        ("滑行", "hua xing"),
        ("输入", "shu ru"),
        ("技术", "ji shu"),
        ("通过", "tong guo"),
        ("手指", "shou zhi"),
        ("在", "zai"),
        ("键盘", "jian pan"),
        ("上", "shang"),
        ("连续", "lian xu"),
        ("滑动", "hua dong"),
        ("实现", "shi xian"),
        ("快速", "kuai su"),
        ("提高", "ti gao"),
        // 词库把 率(lǜ) 归一化成 "lu"，测试读音须与词库一致
        ("效率", "xiao lu"),
        // 第五段
        ("未来", "wei lai"),
        ("人工", "ren gong"),
        ("智能", "zhi neng"),
        ("将", "jiang"),
        ("进一步", "jin yi bu"),
        ("提升", "ti sheng"),
        ("用户", "yong hu"),
        ("体验", "ti yan"),
        ("实现", "shi xian"),
        ("更加", "geng jia"),
        ("自然", "zi ran"),
        ("流畅", "liu chang"),
        ("沟通", "gou tong"),
    ];

    raw.iter()
        .map(|&(text, reading)| TestWord {
            text: text.to_owned(),
            reading: reading.to_owned(),
            symbol_path: reading.chars().filter(|c| *c != ' ').collect(),
        })
        .collect()
}

fn lexicon_test_data(pack: &LanguagePack, limit: usize) -> Vec<TestWord> {
    pack.lexicon
        .iter_entries()
        .filter(|e| {
            let syllables = e.reading.split_whitespace().count();
            syllables >= 1
                && syllables <= 3
                && !e.reading.is_empty()
                && e.reading
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b == b' ')
        })
        .take(limit)
        .map(|e| {
            let symbol_path: String = e.reading.chars().filter(|c| *c != ' ').collect();
            TestWord {
                text: e.text.clone(),
                reading: e.reading.clone(),
                symbol_path,
            }
        })
        .collect()
}

fn corpus_long_pinyin_test_data(
    pack: &LanguagePack,
    corpus_path: &PathBuf,
    limit: usize,
    min_symbol_len: usize,
) -> Result<Vec<TestWord>, Box<dyn Error>> {
    let mut by_text: HashMap<String, TestWord> = HashMap::new();
    let mut lexicon_order = Vec::new();
    for entry in pack.lexicon.iter_entries() {
        let char_count = entry.text.chars().count();
        let syllables = entry.reading.split_whitespace().count();
        if !(3..=4).contains(&char_count)
            || syllables != char_count
            || !entry.text.chars().all(is_cjk_unified)
            || !entry
                .reading
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b == b' ')
        {
            continue;
        }
        let symbol_path: String = entry.reading.chars().filter(|c| *c != ' ').collect();
        if !(min_symbol_len..=28).contains(&symbol_path.len()) {
            continue;
        }
        if !by_text.contains_key(&entry.text) {
            lexicon_order.push(entry.text.clone());
            by_text.insert(
                entry.text.clone(),
                TestWord {
                    text: entry.text.clone(),
                    reading: entry.reading.clone(),
                    symbol_path,
                },
            );
        }
    }

    if by_text.is_empty() || limit == 0 {
        return Ok(Vec::new());
    }

    let mut counts: HashMap<String, u32> = HashMap::new();
    if corpus_path.exists() {
        let wanted = by_text.keys().cloned().collect::<HashSet<_>>();
        let reader = BufReader::new(File::open(corpus_path)?);
        for line in reader.lines() {
            let chars = line?
                .chars()
                .filter(|ch| is_cjk_unified(*ch))
                .collect::<Vec<_>>();
            for width in [3usize, 4] {
                if chars.len() < width {
                    continue;
                }
                for window in chars.windows(width) {
                    let text = window.iter().collect::<String>();
                    if wanted.contains(&text) {
                        *counts.entry(text).or_default() += 1;
                    }
                }
            }
        }
    }

    let mut ranked = counts.into_iter().collect::<Vec<_>>();
    ranked.sort_by(|(left, left_count), (right, right_count)| {
        right_count.cmp(left_count).then_with(|| left.cmp(right))
    });

    let mut out = Vec::new();
    let mut used = HashSet::new();
    for (text, _count) in ranked {
        if let Some(word) = by_text.get(&text) {
            out.push(TestWord {
                text: word.text.clone(),
                reading: word.reading.clone(),
                symbol_path: word.symbol_path.clone(),
            });
            used.insert(text);
            if out.len() >= limit {
                return Ok(out);
            }
        }
    }

    for text in lexicon_order {
        if used.contains(&text) {
            continue;
        }
        if let Some(word) = by_text.get(&text) {
            out.push(TestWord {
                text: word.text.clone(),
                reading: word.reading.clone(),
                symbol_path: word.symbol_path.clone(),
            });
            if out.len() >= limit {
                break;
            }
        }
    }

    Ok(out)
}

fn corpus_chunk_pinyin_test_data(
    pack: &LanguagePack,
    corpus_path: &PathBuf,
    limit: usize,
    min_symbol_len: usize,
) -> Result<Vec<TestWord>, Box<dyn Error>> {
    if limit == 0 || !corpus_path.exists() {
        return Ok(Vec::new());
    }

    let char_readings = single_char_readings(pack);
    let mut counts: HashMap<String, u32> = HashMap::new();
    let reader = BufReader::new(File::open(corpus_path)?);
    for line in reader.lines() {
        let chars = line?
            .chars()
            .filter(|ch| is_cjk_unified(*ch))
            .collect::<Vec<_>>();
        for width in [3usize, 4] {
            if chars.len() < width {
                continue;
            }
            for window in chars.windows(width) {
                if window.iter().all(|ch| char_readings.contains_key(ch)) {
                    let text = window.iter().collect::<String>();
                    *counts.entry(text).or_default() += 1;
                }
            }
        }
    }

    let mut ranked = counts.into_iter().collect::<Vec<_>>();
    ranked.sort_by(|(left, left_count), (right, right_count)| {
        right_count.cmp(left_count).then_with(|| left.cmp(right))
    });

    let mut out = Vec::new();
    for (text, _count) in ranked {
        let Some(reading) = reading_from_chars(&text, &char_readings) else {
            continue;
        };
        let symbol_path = reading.chars().filter(|c| *c != ' ').collect::<String>();
        if !(min_symbol_len..=28).contains(&symbol_path.len()) {
            continue;
        }
        out.push(TestWord {
            text,
            reading,
            symbol_path,
        });
        if out.len() >= limit {
            break;
        }
    }
    Ok(out)
}

fn single_char_readings(pack: &LanguagePack) -> HashMap<char, String> {
    let mut out = HashMap::new();
    for entry in pack.lexicon.iter_entries() {
        let mut chars = entry.text.chars();
        let Some(ch) = chars.next() else { continue };
        if chars.next().is_some() || !is_cjk_unified(ch) {
            continue;
        }
        if entry.reading.split_whitespace().count() != 1 {
            continue;
        }
        out.entry(ch).or_insert_with(|| entry.reading.clone());
    }
    out
}

fn reading_from_chars(text: &str, char_readings: &HashMap<char, String>) -> Option<String> {
    text.chars()
        .map(|ch| char_readings.get(&ch).cloned())
        .collect::<Option<Vec<_>>>()
        .map(|parts| parts.join(" "))
}

fn is_cjk_unified(ch: char) -> bool {
    ('\u{4e00}'..='\u{9fff}').contains(&ch)
}
