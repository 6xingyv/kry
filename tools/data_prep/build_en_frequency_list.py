#!/usr/bin/env python3
"""Build the clean English frequency list used by the en-word language pack.

Source = OpenSubtitles 2018 word frequencies (hermitdave/FrequencyWords, MIT)
intersected with a validity dictionary (dwyl/english-words words_alpha) to drop
OCR/typo junk (itt, ist, tio, pwr, ...). Output is `word<TAB>count` sorted by
count, written to tools/data/en-frequency.tsv. The pack builder log-compresses
the counts into lexicon weights (see train_language_packs.py / build_en_lexicon).

Re-run only to refresh the source; the produced TSV is committed so the pack is
reproducible offline.

Usage: python3 tools/build_en_frequency_list.py [--min-count 12] [--max-len 20]
"""
import argparse
import re
import sys
import urllib.request
from pathlib import Path

FREQ_URL = "https://raw.githubusercontent.com/hermitdave/FrequencyWords/master/content/2018/en/en_full.txt"
DICT_URL = "https://raw.githubusercontent.com/dwyl/english-words/master/words_alpha.txt"
OUT = Path(__file__).resolve().parents[1] / "data" / "en-frequency.tsv"


def fetch(url: str) -> str:
    print(f"fetching {url}", file=sys.stderr)
    with urllib.request.urlopen(url, timeout=120) as resp:
        return resp.read().decode("utf-8", errors="ignore")


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--min-count", type=int, default=12)
    ap.add_argument("--max-len", type=int, default=20)
    # A high-frequency token is kept even if absent from the dictionary (ok, tv…).
    ap.add_argument("--keep-above", type=int, default=50000)
    args = ap.parse_args()

    valid = {w.strip().lower() for w in fetch(DICT_URL).split() if w.strip()}
    print(f"validity dict: {len(valid):,} words", file=sys.stderr)

    rx = re.compile(r"^[a-z]+$")
    kept = []
    for line in fetch(FREQ_URL).splitlines():
        parts = line.split()
        if len(parts) != 2:
            continue
        word, count = parts
        if not rx.match(word) or not (1 <= len(word) <= args.max_len):
            continue
        try:
            count = int(count)
        except ValueError:
            continue
        if count < args.min_count:
            continue
        if word in valid or count >= args.keep_above:
            kept.append((word, count))

    kept.sort(key=lambda wc: -wc[1])
    OUT.parent.mkdir(parents=True, exist_ok=True)
    with OUT.open("w", encoding="utf-8", newline="\n") as out:
        for word, count in kept:
            out.write(f"{word}\t{count}\n")
    print(f"wrote {len(kept):,} words -> {OUT}", file=sys.stderr)


if __name__ == "__main__":
    main()
