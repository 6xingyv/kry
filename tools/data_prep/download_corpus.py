from __future__ import annotations

import argparse
import json
import sys
import urllib.request
from pathlib import Path


WORKSPACE = Path(__file__).resolve().parents[2]
LOCAL_TOOL_PACKAGES = WORKSPACE / ".cache" / "python-tools"
if LOCAL_TOOL_PACKAGES.exists():
    sys.path.insert(0, str(LOCAL_TOOL_PACKAGES))

CORPUS_DIR = WORKSPACE / "datasets" / "corpus"

WIKITEXT_103_PARQUET = {
    "train": [
        f"https://huggingface.co/datasets/Salesforce/wikitext/resolve/main/wikitext-103-raw-v1/train-0000{i}-of-00002.parquet"
        for i in range(2)
    ],
    "validation": [
        "https://huggingface.co/datasets/Salesforce/wikitext/resolve/main/wikitext-103-raw-v1/validation-00000-of-00001.parquet",
    ],
    "test": [
        "https://huggingface.co/datasets/Salesforce/wikitext/resolve/main/wikitext-103-raw-v1/test-00000-of-00001.parquet",
    ],
}


def download_file(url: str, dest: Path) -> None:
    if dest.exists() and dest.stat().st_size > 0:
        return
    dest.parent.mkdir(parents=True, exist_ok=True)
    tmp = dest.with_suffix(dest.suffix + ".tmp")
    if tmp.exists():
        tmp.unlink()
    request = urllib.request.Request(
        url, headers={"User-Agent": "kry-keyboard-corpus-downloader/1.0"}
    )
    print(f"  downloading {url}")
    with urllib.request.urlopen(request) as resp, tmp.open("wb") as f:
        while True:
            chunk = resp.read(1024 * 1024)
            if not chunk:
                break
            f.write(chunk)
    tmp.replace(dest)


def parquet_to_jsonl(
    parquet_files: list[Path],
    output: Path,
    source_tag: str,
    text_column: str = "text",
    skip_headings: bool = True,
) -> int:
    import pyarrow.parquet as pq

    rows = 0
    with output.open("w", encoding="utf-8", newline="\n") as out:
        for pf in parquet_files:
            table = pq.read_table(pf, columns=[text_column])
            for text in table.column(text_column).to_pylist():
                text = str(text).strip()
                if not text:
                    continue
                if skip_headings and text.startswith("="):
                    continue
                out.write(
                    json.dumps(
                        {"source": source_tag, "text": text},
                        ensure_ascii=False,
                        separators=(",", ":"),
                    )
                    + "\n"
                )
                rows += 1
    return rows


def download_en_wikitext_103(output_dir: Path) -> None:
    print("[en-wikitext-103] downloading parquet files...")
    parquet_dir = output_dir / "_parquet" / "en-wikitext-103"
    all_files: list[Path] = []
    for split, urls in WIKITEXT_103_PARQUET.items():
        for i, url in enumerate(urls):
            dest = parquet_dir / f"{split}-{i}.parquet"
            download_file(url, dest)
            all_files.append(dest)

    jsonl = output_dir / "en-wikitext-103.jsonl"
    rows = parquet_to_jsonl(
        all_files, jsonl, "Salesforce/wikitext:wikitext-103-raw-v1"
    )
    print(f"[en-wikitext-103] output={jsonl} rows={rows}")


def download_zh_wikipedia(output_dir: Path) -> None:
    print("[zh-wikipedia] downloading via HuggingFace datasets API...")
    try:
        from datasets import load_dataset
    except ImportError:
        print(
            "  ERROR: 'datasets' package not installed. "
            "Install with: pip install datasets",
            file=sys.stderr,
        )
        return

    ds = load_dataset("wikimedia/wikipedia", "20231101.zh", split="train")
    jsonl = output_dir / "zh-wikipedia.jsonl"
    rows = 0
    with jsonl.open("w", encoding="utf-8", newline="\n") as out:
        for item in ds:
            text = str(item["text"]).strip()
            if not text:
                continue
            out.write(
                json.dumps(
                    {"source": "wikimedia/wikipedia:20231101.zh", "text": text},
                    ensure_ascii=False,
                    separators=(",", ":"),
                )
                + "\n"
            )
            rows += 1
    print(f"[zh-wikipedia] output={jsonl} rows={rows}")


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Download corpus data for LM training"
    )
    parser.add_argument("--all", action="store_true", help="Download all corpora")
    parser.add_argument(
        "--en-wiki-103",
        action="store_true",
        help="Download English WikiText-103",
    )
    parser.add_argument(
        "--zh-wiki",
        action="store_true",
        help="Download Chinese Wikipedia (requires 'datasets' package)",
    )
    parser.add_argument(
        "--output-dir",
        type=Path,
        default=CORPUS_DIR,
        help="Output directory for corpus files",
    )
    args = parser.parse_args()

    if not (args.all or args.en_wiki_103 or args.zh_wiki):
        parser.print_help()
        sys.exit(1)

    args.output_dir.mkdir(parents=True, exist_ok=True)

    if args.all or args.en_wiki_103:
        download_en_wikitext_103(args.output_dir)
    if args.all or args.zh_wiki:
        download_zh_wikipedia(args.output_dir)

    print("done.")


if __name__ == "__main__":
    main()
