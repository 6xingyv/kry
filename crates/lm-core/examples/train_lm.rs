//! Native candle trainer for the MiniGPT character LM — no PyTorch.
//!
//! Usage:
//!   cargo run --release -p lm-core --example train_lm -- <corpus.txt> <steps> [ctx] [batch]
//!
//! Corpus is one sentence per line (Leipzig format `id<TAB>sentence` is handled).
//! Rebuilds assets/lm/tokenizer.json + config.json from the corpus vocab and
//! writes the trained weights to assets/lm/model.safetensors.

use std::collections::HashMap;
use std::path::Path;

use candle_core::{DType, Device, Tensor};
use candle_nn::{AdamW, Optimizer, ParamsAdamW, VarBuilder, VarMap, loss};
use lm_core::{CharacterTokenizer, MiniGptConfig, MiniGptModel};

/// Pick the training device: Apple GPU when built with `--features metal`,
/// otherwise CPU. Falls back to CPU if Metal init fails.
fn select_device() -> Device {
    #[cfg(feature = "metal")]
    {
        match Device::new_metal(0) {
            Ok(device) => {
                eprintln!("training on Metal (Apple GPU)");
                return device;
            }
            Err(err) => eprintln!("Metal unavailable ({err}); falling back to CPU"),
        }
    }
    eprintln!("training on CPU");
    Device::Cpu
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let corpus_path = args.get(1).cloned().unwrap_or_else(|| {
        "datasets/corpus/zho_news_2020_300K/zho_news_2020_300K-sentences.txt".to_owned()
    });
    let steps: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(4000);
    let ctx_len: usize = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(64);
    let batch_size: usize = args.get(4).and_then(|s| s.parse().ok()).unwrap_or(32);
    // Per-language expert LMs train to their own dir (assets/lm-zh, assets/lm-en).
    let out_dir_arg = args.get(5).cloned().unwrap_or_else(|| "assets/lm".to_owned());
    let out_dir = Path::new(&out_dir_arg);
    std::fs::create_dir_all(out_dir)?;
    let vocab_cap = 8192usize;
    let peak_lr = 6e-4;
    let warmup = 200usize;

    // ── 1. read corpus (Leipzig: `id<TAB>sentence`) ──
    let raw = std::fs::read_to_string(&corpus_path)?;
    let sentences: Vec<&str> = raw
        .lines()
        .map(|l| l.split_once('\t').map(|(_, s)| s).unwrap_or(l).trim())
        .filter(|s| !s.is_empty())
        .collect();
    println!("sentences: {}", sentences.len());

    // ── 2. build vocab from char frequency ──
    let mut freq: HashMap<char, u64> = HashMap::new();
    for s in &sentences {
        for ch in s.chars() {
            *freq.entry(ch).or_default() += 1;
        }
    }
    let mut ranked: Vec<(char, u64)> = freq.into_iter().collect();
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    ranked.truncate(vocab_cap - 3);
    let vocab_chars: Vec<char> = ranked.iter().map(|(c, _)| *c).collect();
    let tokenizer = CharacterTokenizer::from_vocab(&vocab_chars);
    let vocab_size = tokenizer.vocab_size();
    println!("unique chars used (vocab): {vocab_size}");
    save_tokenizer_json(&out_dir.join("tokenizer.json"), &vocab_chars)?;

    let config = MiniGptConfig {
        vocab_size,
        n_layers: 4,
        n_heads: 4,
        hidden_size: 256,
        intermediate_size: 512,
        max_seq_len: 256,
    };
    std::fs::write(
        out_dir.join("config.json"),
        serde_json::to_string_pretty(&config)?,
    )?;

    // ── 3. flatten to a token stream with <eos> between sentences ──
    let eos = 2u32;
    let mut stream: Vec<u32> = Vec::new();
    for s in &sentences {
        let ids = tokenizer.encode(s);
        if ids.is_empty() {
            continue;
        }
        stream.extend(ids);
        stream.push(eos);
    }
    println!("token stream length: {}", stream.len());
    if stream.len() < ctx_len + 2 {
        return Err("corpus too small".into());
    }
    let max_start = stream.len() - ctx_len - 1;

    // ── 4. model + AdamW (fresh VarMap = trainable params) ──
    let device = select_device();
    let varmap = VarMap::new();
    let vb = VarBuilder::from_varmap(&varmap, DType::F32, &device);
    let model = MiniGptModel::new(config.clone(), tokenizer, vb)?;
    let params = ParamsAdamW {
        lr: peak_lr,
        weight_decay: 0.01,
        ..Default::default()
    };
    let mut opt = AdamW::new(varmap.all_vars(), params)?;
    println!(
        "training: steps={steps} ctx={ctx_len} batch={batch_size} peak_lr={peak_lr} warmup={warmup} (≈3M params, baseline loss ln(V)={:.2})",
        (vocab_size as f32).ln()
    );

    // linear warmup → cosine decay to 10% of peak
    let lr_at = |step: usize| -> f64 {
        if step < warmup {
            peak_lr * (step as f64 + 1.0) / warmup as f64
        } else {
            let progress = (step - warmup) as f64 / (steps - warmup).max(1) as f64;
            let cos = 0.5 * (1.0 + (std::f64::consts::PI * progress).cos());
            peak_lr * (0.1 + 0.9 * cos)
        }
    };

    // ── 5. train ──
    let mut rng = 0x1234_5678_9abc_def0u64;
    let mut running = 0f32;
    let mut count = 0u32;
    for step in 1..=steps {
        opt.set_learning_rate(lr_at(step - 1));
        let mut inp = Vec::with_capacity(batch_size * ctx_len);
        let mut tgt = Vec::with_capacity(batch_size * ctx_len);
        for _ in 0..batch_size {
            rng = rng
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            let s = (rng >> 33) as usize % max_start;
            inp.extend_from_slice(&stream[s..s + ctx_len]);
            tgt.extend_from_slice(&stream[s + 1..s + ctx_len + 1]);
        }
        let input = Tensor::from_vec(inp, (batch_size, ctx_len), &device)?;
        let logits = model
            .forward_batch(&input)?
            .reshape((batch_size * ctx_len, vocab_size))?;
        let target = Tensor::from_vec(tgt, (batch_size * ctx_len,), &device)?;
        let l = loss::cross_entropy(&logits, &target)?;
        opt.backward_step(&l)?;

        running += l.to_scalar::<f32>()?;
        count += 1;
        if step % 50 == 0 {
            println!("step {step}/{steps}  loss {:.4}", running / count as f32);
            running = 0.0;
            count = 0;
        }
        if step % 1000 == 0 {
            varmap.save(out_dir.join("model.safetensors"))?;
            println!("  …checkpoint saved at step {step}");
        }
    }
    varmap.save(out_dir.join("model.safetensors"))?;
    println!("done. saved {}/model.safetensors", out_dir.display());
    Ok(())
}

fn save_tokenizer_json(path: &Path, vocab_chars: &[char]) -> std::io::Result<()> {
    use std::fmt::Write as _;
    // Format matches CharacterTokenizer::from_json_path: {vocab_size, char_to_id, special_tokens}
    let mut char_to_id = String::from("{");
    for (i, ch) in vocab_chars.iter().enumerate() {
        if i > 0 {
            char_to_id.push(',');
        }
        let key = ch.to_string();
        let escaped = serde_json::to_string(&key).unwrap();
        let _ = write!(char_to_id, "{}:{}", escaped, 3 + i);
    }
    char_to_id.push('}');
    let json = format!(
        "{{\n  \"vocab_size\": {},\n  \"special_tokens\": {{\"<pad>\": 0, \"<unk>\": 1, \"<eos>\": 2}},\n  \"char_to_id\": {}\n}}\n",
        vocab_chars.len() + 3,
        char_to_id
    );
    std::fs::write(path, json)
}
