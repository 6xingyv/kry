#!/usr/bin/env bash
# Fetch a Chinese training corpus WITHOUT the `datasets`/`pyarrow` libraries.
#
# The HuggingFace Wikipedia parquet stores its `text` column as plain UTF-8, so a
# stdlib regex extracts clean sentences straight from the downloaded bytes. The HF
# CDN is fast (~16 MB/s); the Leipzig mirror is not. Produces datasets/corpus/zh/zh-wiki.txt.
#
# Usage: tools/fetch_zh_corpus.sh [num_shards]   (default 6 = full dump)
set -euo pipefail
cd "$(dirname "$0")/../.."
SHARDS="${1:-6}"
OUT=datasets/corpus/zh
mkdir -p "$OUT"

base="https://huggingface.co/datasets/wikimedia/wikipedia/resolve/main/20231101.zh"
for i in $(seq 0 $((SHARDS-1))); do
  f="$OUT/zh-wiki-shard$i.parquet"
  if [ ! -s "$f" ]; then
    printf 'downloading shard %d/%d…\n' "$i" "$SHARDS"
    tmp="$f.part"
    rm -f "$tmp"
    curl -fL --retry 3 -o "$tmp" "$base/train-0000$i-of-00006.parquet"
    mv "$tmp" "$f"
  fi
done

echo "extracting sentences…"
python3 - "$OUT" <<'PY'
import re, glob, sys
out_dir = sys.argv[1]
# Pure CJK ideographs + Chinese punctuation only: parquet framing bytes (latin /
# control) break runs apart and get filtered, leaving clean sentences.
seg_re = re.compile(r'[一-鿿、。，！？；：“”‘’（）《》]{8,}')
split_re = re.compile(r'[。！？\n]')
seen=set(); out=[]
for fp in sorted(glob.glob(f'{out_dir}/zh-wiki-shard*.parquet')):
    text = open(fp,'rb').read().decode('utf-8','ignore')
    for seg in seg_re.findall(text):
        for s in split_re.split(seg):
            s=s.strip('、，；： ')
            zh=sum(1 for c in s if '一'<=c<='鿿')
            if zh>=5 and len(s)<=80 and zh/len(s)>=0.85 and s not in seen:
                seen.add(s); out.append(s)
open(f'{out_dir}/zh-wiki.txt','w').write('\n'.join(out))
print(f"wrote {len(out)} sentences, {sum(len(s) for s in out)} chars, {len(set(''.join(out)))} uniq chars")
PY
echo "done → $OUT/zh-wiki.txt"
echo "train with: cargo run --release -p lm-core --example train_lm -- $OUT/zh-wiki.txt 12000 64 32"
