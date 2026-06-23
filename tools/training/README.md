# LM 训练（两条路径）

产物都是一个目录 `{config.json, tokenizer.json, model.safetensors}`，放进
`assets/lm`（中文专家）或 `assets/lm-en`（英文专家）。两条路径等价，按环境选用。

## 路径 A — PyTorch（CUDA / Apple MPS / CPU）
需要 `torch`。`train_lm.py` 自动选设备（`_best_device`：CUDA → MPS → CPU），
所以 **NVIDIA(CUDA) 与 Apple(MPS) 都走这条**。

```bash
# 1) 字符表
python tools/training/train_tokenizer.py --corpus datasets/corpus/zh/zh-wiki.txt --out assets/lm/tokenizer.json
# 2) 训练
python tools/training/train_lm.py train --corpus datasets/corpus/zh/zh-wiki.txt --out assets/lm
# 3) 量化（部署可选）→ assets/lm/model-q8.safetensors
python tools/training/train_lm.py quantize
```

## 路径 B — candle（Apple Metal / CPU，无需 torch）
纯 Rust，环境没有 torch 时用。第 5 个参数是输出目录（各语言专家）。

```bash
# Metal（Apple GPU，实测 ≈3× CPU）
cargo run --release -p lm-core --example train_lm --features metal -- \
  datasets/corpus/zh/zh-wiki.txt 4000 64 48 assets/lm        # 中文
cargo run --release -p lm-core --example train_lm --features metal -- \
  datasets/corpus/en/en-clean.txt 4000 64 48 assets/lm-en    # 英文专家
# CPU：去掉 --features metal
```
参数：`<corpus> <steps> <ctx_len> <batch> <out_dir>`。candle 模型定义见
`crates/lm-core/src/model.rs`。

## 调参 / 评估
`tools/training/lm_sweep.sh [lm_dir]` —— 扫 swipe LM 的 `KRY_LM_ALPHA`/`KRY_LM_SCALE`
对 `validate_lm_context` 的影响。

## 语料
`datasets/corpus/{zh,en}/`（见各自 README 的来源/许可/用途）。
