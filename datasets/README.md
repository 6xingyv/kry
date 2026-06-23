# datasets/

原始/派生语料，按语言分类。每个语言目录下有自己的 `README.md`，记录每个数据集的
下载时间、来源地址、许可证与用途。

```text
datasets/
  corpus/
    zh/   中文语料（维基百科）
    en/   英文语料（WikiText-2 + 清洗版）
```

注意：
- 本目录**不随发布包打包**（体积大、许可证要求署名）。
- 词频派生资源（如英文 OpenSubtitles∩词典清单）放在 `tools/data/`，并由
  `tools/build_en_frequency_list.py` 复现，不在此目录。
- FUTO 滑行训练数据（观测模型）若在本地，放 `datasets/futo-swipe/`（见其 README）。
