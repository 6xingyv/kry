# tools/

```text
tools/
  train_language_packs.py   语言包构建器：schema FST + 词库 + 频率表 + 上下文模型 → assets/language-packs
  training/                 语言模型(LM)训练 —— 两条路径：PyTorch(CUDA/MPS) 与 candle(Metal/CPU)
  data_prep/                语料下载 + 词频清单构建
  data/                     派生构建资源（en-frequency.tsv，由 data_prep 生成、被 train_language_packs 读取）
```

所有脚本均从**仓库根目录**运行（路径相对根目录解析）。各子目录有自己的 README。
