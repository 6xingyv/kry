# 英文语料 (en)

## en-wikitext2.txt
- **下载时间**：2026-06-22
- **来源**：WikiText-2（raw），经 `pytorch/examples` 镜像获取；原始数据集
  Salesforce/WikiText，源自英文维基百科。
  https://raw.githubusercontent.com/pytorch/examples/main/word_language_model/data/wikitext-2/train.txt
- **许可证**：CC BY-SA 3.0（源自维基百科）。
- **用途**：英文 char-level LM 训练的原始语料。

## en-clean.txt（派生）
- **来源**：由 `en-wikitext2.txt` 清洗而来——去除 `<unk>`、`@-@/@.@/@,@` 标记与
  `= 标题 =` 行，仅保留长度≥20 且 ASCII 占比 >95% 的行。
- **许可证**：同上（CC BY-SA 3.0，派生作品）。
- **用途**：英文专家 LM（`assets/lm-en`）的训练输入。
- **复现**：见 `tools/training/` 下的清洗步骤（README）。
