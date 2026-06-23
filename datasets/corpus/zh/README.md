# 中文语料 (zh)

## zh-wiki.txt
- **下载时间**：2026-06-21
- **来源**：HuggingFace `wikimedia/wikipedia`，快照 `20231101.zh`（parquet 分片，
  经 `tools/fetch_zh_corpus.sh` 正则抽取纯 CJK 句子）。
  https://huggingface.co/datasets/wikimedia/wikipedia
- **许可证**：CC BY-SA 4.0 / GFDL（维基百科条款）。再分发需保留署名与同协议。
- **用途**：
  - 中文 char-level LM 训练（`assets/lm`，PyTorch `train_lm.py` 或 candle
    `train_lm.rs`）。
  - `train_language_packs.py` 的 `--zh-context-corpus` / `--zh-phrase-corpus`
    （上下文续写 + 3–4 字短语扩充）。
