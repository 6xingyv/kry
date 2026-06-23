use std::path::Path;

use candle_core::Device;
use lm_core::{CharacterTokenizer, LanguageModel, MiniGptConfig, MiniGptModel};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let lm_root = Path::new(args.get(1).map(String::as_str).unwrap_or("assets/lm"));
    let tokenizer = CharacterTokenizer::from_json_path(&lm_root.join("tokenizer.json"))?;
    let config: MiniGptConfig =
        serde_json::from_slice(&std::fs::read(lm_root.join("config.json"))?)?;
    let vocab_size = config.vocab_size;
    let model = MiniGptModel::load(
        &lm_root.join("model.safetensors"),
        config,
        tokenizer,
        &Device::Cpu,
    )?;

    let uniform = -(vocab_size as f32).ln(); // 随机模型的每-token 基线

    // probes from args, else default Chinese set
    let default_probes = ["中国", "我们", "你好", "谢谢", "语言模型", "今天天气"];
    let owned: Vec<String> = if args.len() > 2 {
        args[2..].to_vec()
    } else {
        default_probes.iter().map(|s| s.to_string()).collect()
    };
    let probes: Vec<&str> = owned.iter().map(String::as_str).collect();
    println!("基线（随机模型每 token logP）≈ {uniform:.2}");
    println!("── score_sequence / 每 token 平均 logP ──");
    for p in probes {
        let ids = model.encode(p);
        if ids.len() < 2 {
            println!("  {p}: <token 不足>");
            continue;
        }
        let total = model.score_sequence(&ids);
        let per = total / (ids.len() - 1) as f32;
        println!(
            "  {p:<8} tokens={:<2} total={total:.2} per_token={per:.2}",
            ids.len()
        );
    }

    // 续写预测：给定前缀，top-5 下一字
    println!("── 续写 top-5 ──");
    for ctx in ["我爱中", "天气很", "语言模"] {
        let ids = model.encode(ctx);
        let top = model.predict_top_k(&ids, 5);
        let s: Vec<String> = top
            .iter()
            .map(|(id, lp)| format!("{}({lp:.1})", model.decode(&[*id])))
            .collect();
        println!("  {ctx} → {}", s.join(" "));
    }

    Ok(())
}
