# 数据准备

从仓库根目录运行。产物落到 `datasets/corpus/<lang>/` 或 `tools/data/`。

| 脚本 | 作用 | 产物 |
|------|------|------|
| `fetch_zh_corpus.sh [shards]` | 下载中文维基 parquet 分片并抽取纯 CJK 句子 | `datasets/corpus/zh/zh-wiki.txt` |
| `download_wikitext_context.py` | 下载英文 WikiText | `datasets/corpus/en/`（`--output-dir`） |
| `download_corpus.py` | 通用语料下载（HuggingFace 中/英） | `--output-dir`（建议 `datasets/corpus/<lang>`） |
| `build_en_frequency_list.py` | OpenSubtitles 词频 ∩ 词典 → 干净英文词频清单（英文词库源） | `tools/data/en-frequency.tsv` |

许可证/来源记录见 `datasets/corpus/<lang>/README.md`。`en-frequency.tsv` 的来源与
过滤规则见脚本头部注释（OpenSubtitles 2018 MIT ∩ dwyl words_alpha）。
