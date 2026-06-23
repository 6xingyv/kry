from __future__ import annotations

import argparse
import gzip
import json
import math
import subprocess
import re
import statistics
import sys
import time
import unicodedata
from collections import Counter, defaultdict
from dataclasses import asdict, dataclass
from pathlib import Path

import yaml


WORKSPACE = Path(__file__).resolve().parents[1]
DEFAULT_KRY_ROOT = Path(r"E:\mocha\kry")
DEFAULT_FUTO = DEFAULT_KRY_ROOT / "datasets" / "futo-swipe" / "train.jsonl"
DEFAULT_RIME = DEFAULT_KRY_ROOT / "isolated" / "rime-wanxiang"
DEFAULT_ENGLISH = DEFAULT_KRY_ROOT / "isolated" / "top-english-wordlists"
# Clean, committed English frequency list (OpenSubtitles ∩ dictionary). Preferred
# over the raw wordlist: real frequencies + no OCR/typo junk. See
# tools/build_en_frequency_list.py for provenance.
DEFAULT_EN_FREQ = WORKSPACE / "tools" / "data" / "en-frequency.tsv"

PINYIN_RE = re.compile(r"^[a-züv:āáǎàēéěèīíǐìōóǒòūúǔùǖǘǚǜńňǹḿ ]+$", re.I)
EN_CONTEXT_TOKEN_RE = re.compile(r"[A-Za-z]+(?:'[A-Za-z]+)?")
EN_APOSTROPHE_JOIN_RE = re.compile(r"\b([A-Za-z]+)\s*'\s*(s|t|m|re|ve|ll|d)\b", re.I)


@dataclass(frozen=True)
class BuildSummary:
    artifact: str
    entries: int
    output: str
    elapsed_ms: int
    source: str


SCHEMA_REGISTRY = [
    {
        "schema": "zh-hans-pinyin-full",
        "profile": "zh-qwerty",
        "language_pack": "zh-hans-pinyin-full",
        "required_components": ["schema_fst", "lexicon", "frequency_table", "context_model"],
        "data": ["rime-wanxiang"],
    },
    {
        "schema": "en-word",
        "profile": "en-qwerty",
        "language_pack": "en-word",
        "required_components": ["schema_fst", "lexicon", "frequency_table", "context_model"],
        "data": ["top-english-wordlists"],
    },
    {
        "schema": "es-word",
        "profile": "en-es-qwerty",
        "language_pack": "es-word",
        "required_components": ["schema_fst", "lexicon", "frequency_table", "context_model"],
        "data": [],
        "reason": "missing Spanish lexicon in provided local data",
    },
    {
        "schema": "ru-cyrillic",
        "profile": "ru-native",
        "language_pack": "ru-cyrillic",
        "required_components": ["schema_fst", "lexicon", "frequency_table", "context_model", "morphology"],
        "data": [],
        "reason": "missing Russian Cyrillic lexicon in provided local data",
    },
    {
        "schema": "ru-translit",
        "profile": "ru-translit",
        "language_pack": "ru-translit",
        "required_components": ["transliteration_table", "lexicon", "frequency_table", "context_model"],
        "data": [],
        "reason": "missing Russian lexicon in provided local data",
    },
    {
        "schema": "emoji-zh-hans",
        "profile": "zh-qwerty",
        "language_pack": "emoji-zh-hans",
        "required_components": ["alias_table", "lexicon", "frequency_table", "context_model"],
        "data": ["rime-wanxiang/lua/data/emoji.txt"],
    },
    {
        "schema": "emoji-en",
        "profile": "en-qwerty",
        "language_pack": "emoji-en",
        "required_components": ["alias_table", "lexicon", "frequency_table", "context_model"],
        "data": ["rime-wanxiang/lua/data/emoji.txt"],
    },
]


def main() -> None:
    args = parse_args()
    if args.command in {"build", "build-lexicons"}:
        summaries = [
            build_pinyin_pack(args),
            build_english_pack(args),
            build_emoji_pack(args, "emoji-zh-hans", "zh-hans"),
            build_emoji_pack(args, "emoji-en", "en"),
            build_observation_error_model(args),
        ]
        for optional_summary in [
            build_spanish_pack(args),
            build_russian_cyrillic_pack(args),
            build_russian_translit_pack(args),
        ]:
            if optional_summary is not None:
                summaries.append(optional_summary)
        inspect(args)
        if not args.skip_binary_assets:
            compile_binary_assets(args)
            inspect(args)
        print(json.dumps([asdict(summary) for summary in summaries], ensure_ascii=False, indent=2))
    elif args.command in {"inspect", "status"}:
        inspect(args)
    else:
        raise SystemExit(f"unsupported command: {args.command}")


def build_pinyin_pack(args) -> BuildSummary:
    started = time.monotonic()
    entries = load_rime_entries(args.rime_root, args.rime_dict, args.max_entries)
    if args.zh_phrase_corpus and args.max_zh_corpus_phrases > 0:
        phrase_entries = build_zh_corpus_phrase_entries(
            args.zh_phrase_corpus,
            entries,
            args.max_zh_corpus_phrases,
            args.min_zh_corpus_phrase_count,
            args.zh_corpus_phrase_weight_scale,
        )
        entries.extend(phrase_entries)
        print(
            f"[zh-hans-pinyin-full] added {len(phrase_entries)} corpus 3-4 char phrase entries",
            file=sys.stderr,
        )
    out_dir = args.language_pack_dir / "zh-hans-pinyin-full"
    out_dir.mkdir(parents=True, exist_ok=True)

    syllable_ids: dict[str, int] = {}
    reading_weights: Counter[str] = Counter()
    compact_to_readings: dict[str, Counter[str]] = defaultdict(Counter)

    def intern_syllable(value: str) -> int:
        if value not in syllable_ids:
            syllable_ids[value] = len(syllable_ids)
        return syllable_ids[value]

    count = 0
    context_entries = []
    with (out_dir / "lexicon.jsonl").open("w", encoding="utf-8", newline="\n") as handle:
        for entry in entries:
            segments = normalize_pinyin_segments(entry["code"])
            if not segments:
                continue
            reading = " ".join(segments)
            compact = "".join(segments)
            weight = float(entry["weight"])
            reading_weights[reading] += weight
            compact_to_readings[compact][reading] += weight
            record = {
                "text": entry["text"],
                "reading": reading,
                "compact": compact,
                "syllable_ids": [intern_syllable(segment) for segment in segments],
                "weight": weight,
            }
            handle.write(json.dumps(record, ensure_ascii=False, separators=(",", ":")) + "\n")
            context_entries.append((entry["text"], segments, weight))
            count += 1

    syllables = [None] * len(syllable_ids)
    for text, idx in syllable_ids.items():
        syllables[idx] = text
    write_json(out_dir / "syllables.json", syllables)
    write_json(
        out_dir / "schema_fst.json",
        {
            "format": "mocha-schema-fst-v1",
            "schema": "zh-hans-pinyin-full",
            "consumes": "latin symbols",
            "emits": "pinyin reading lattice",
            "delimiter": "'",
            "syllables": syllables,
            "fuzzy_edges": build_pinyin_fuzzy_edges(syllables),
            "ambiguous_compact_readings": {
                compact: [
                    {"reading": reading, "weight": weight}
                    for reading, weight in counter.most_common(args.max_ambiguous_readings)
                ]
                for compact, counter in compact_to_readings.items()
                if len(counter) > 1
            },
        },
    )
    write_frequency_table(out_dir, "zh-hans-pinyin-full", reading_weights)
    zh_continuations = build_zh_lexicon_context_continuations(
        context_entries,
        args.max_context_continuations,
    )
    if args.zh_context_corpus:
        zh_continuations = merge_context_continuation_records(
            zh_continuations
            + build_zh_corpus_context_continuations(
                args.zh_context_corpus,
                context_entries,
                args.max_context_continuations,
                args.context_corpus_lines,
            ),
            args.max_context_continuations,
        )
    write_context_model(
        out_dir,
        "zh-hans-pinyin-full",
        reading_weights,
        "reading",
        continuations=zh_continuations,
    )
    write_manifest(
        out_dir,
        {
            "schema": "zh-hans-pinyin-full",
            "format": "mocha-language-pack-v1",
            "components": {
                "schema_fst": "schema_fst.json",
                "lexicon": "lexicon.jsonl",
                "frequency_table": "frequency.json",
                "context_model": "context_model.json",
                "syllables": "syllables.json",
            },
            "source": str(args.rime_root / f"{args.rime_dict}.dict.yaml"),
            "entries": count,
            "syllables": len(syllables),
        },
    )
    return BuildSummary("zh-hans-pinyin-full", count, str(out_dir), elapsed_ms(started), str(args.rime_root))


def build_english_pack(args) -> BuildSummary:
    started = time.monotonic()
    # Prefer the clean, committed frequency list (real OpenSubtitles counts, no
    # OCR/typo junk). Fall back to the raw rank-ordered wordlist if it is absent.
    freq_weights: dict[str, float] = {}
    if args.english_frequency_list and args.english_frequency_list.exists():
        freq_weights = load_english_frequency_list(args.english_frequency_list, args.max_entries)
        print(f"en-word: using clean frequency list ({len(freq_weights):,} words)")
    if freq_weights:
        entries = list(freq_weights.keys())
    else:
        entries = load_english_entries(args.english_dict_root / args.english_dict_file, args.max_entries)
    base_entry_count = len(entries)
    corpus_word_weights = collect_english_corpus_word_weights(
        args.english_context_corpus or [],
        args.context_corpus_lines,
    )
    corpus_surface_weights = collect_english_corpus_surface_weights(
        args.english_context_corpus or [],
        args.context_corpus_lines,
    )
    # Only augment the vocabulary from the corpus when NOT using the clean list,
    # so corpus OCR noise can't re-pollute it.
    if corpus_word_weights and not freq_weights:
        seen_entries = set(entries)
        for word, count in corpus_word_weights.most_common():
            if word not in seen_entries and len(word) >= 2:
                if count < args.min_corpus_lexicon_count:
                    continue
                entries.append(word)
                seen_entries.add(word)
                if args.max_entries and len(entries) >= args.max_entries:
                    break
    out_dir = args.language_pack_dir / "en-word"
    out_dir.mkdir(parents=True, exist_ok=True)

    word_weights: Counter[str] = Counter()
    apostrophe_readings = {
        reading
        for (reading, surface), count in corpus_surface_weights.items()
        if "'" in surface and count >= args.min_corpus_lexicon_count
    }
    written_entries = 0
    written_surfaces: set[tuple[str, str]] = set()
    with (out_dir / "lexicon.jsonl").open("w", encoding="utf-8", newline="\n") as handle:
        for rank, word in enumerate(entries, start=1):
            if word in apostrophe_readings:
                continue
            if word in freq_weights:
                weight = freq_weights[word]
            elif corpus_word_weights:
                weight = float(corpus_word_weights.get(word, 1.0))
            else:
                weight = float(base_entry_count - rank + 1)
            word_weights[word] += weight
            written_entries += 1
            written_surfaces.add((word, word))
            handle.write(
                json.dumps(
                    {"text": word, "reading": word, "compact": word, "weight": weight},
                    ensure_ascii=False,
                    separators=(",", ":"),
                )
                + "\n"
            )
        for (reading, surface), count in corpus_surface_weights.most_common():
            if surface == reading or count < args.min_corpus_lexicon_count:
                continue
            if (reading, surface) in written_surfaces:
                continue
            weight = float(count)
            word_weights[reading] += weight
            written_entries += 1
            written_surfaces.add((reading, surface))
            handle.write(
                json.dumps(
                    {"text": surface, "reading": reading, "compact": reading, "weight": weight},
                    ensure_ascii=False,
                    separators=(",", ":"),
                )
                + "\n"
            )

    write_json(
        out_dir / "schema_fst.json",
        {
            "format": "mocha-schema-fst-v1",
            "schema": "en-word",
            "consumes": "latin symbols",
            "emits": "english word spelling lattice",
            "alphabet": "abcdefghijklmnopqrstuvwxyz",
            "raw_latin_fallback": True,
            "typo_model": {"kind": "neighbor-edit", "max_edits_default": 1},
        },
    )
    write_frequency_table(out_dir, "en-word", word_weights)
    write_context_model(
        out_dir,
        "en-word",
        word_weights,
        "word",
        continuations=build_english_context_continuations(
            args.english_context_corpus or [],
            args.max_context_continuations,
            args.context_corpus_lines,
        ),
    )
    write_manifest(
        out_dir,
        {
            "schema": "en-word",
            "format": "mocha-language-pack-v1",
            "components": {
                "schema_fst": "schema_fst.json",
                "lexicon": "lexicon.jsonl",
                "frequency_table": "frequency.json",
                "context_model": "context_model.json",
            },
            "source": str(args.english_dict_root / args.english_dict_file),
            "entries": written_entries,
        },
    )
    return BuildSummary("en-word", written_entries, str(out_dir), elapsed_ms(started), str(args.english_dict_root))


def build_emoji_pack(args, schema: str, language: str) -> BuildSummary:
    started = time.monotonic()
    source = args.rime_root / "lua" / "data" / "emoji.txt"
    aliases = load_emoji_aliases(source, language)
    out_dir = args.language_pack_dir / schema
    out_dir.mkdir(parents=True, exist_ok=True)

    alias_weights: Counter[str] = Counter()
    with (out_dir / "lexicon.jsonl").open("w", encoding="utf-8", newline="\n") as handle:
        for rank, (alias, outputs) in enumerate(aliases, start=1):
            weight = 1.0 / rank
            alias_weights[alias] += weight
            for output in outputs:
                handle.write(
                    json.dumps(
                        {"text": output, "reading": alias, "compact": alias, "weight": weight},
                        ensure_ascii=False,
                        separators=(",", ":"),
                    )
                    + "\n"
                )

    write_json(
        out_dir / "alias_table.json",
        {
            "format": "mocha-emoji-alias-table-v1",
            "schema": schema,
            "source": str(source),
            "language": language,
            "aliases": [
                {"alias": alias, "outputs": outputs, "weight": 1.0 / rank}
                for rank, (alias, outputs) in enumerate(aliases, start=1)
            ],
            "personal_history": "excluded: PersonalKnowledge overlay",
        },
    )
    write_frequency_table(out_dir, schema, alias_weights)
    write_context_model(out_dir, schema, alias_weights, "alias")
    write_manifest(
        out_dir,
        {
            "schema": schema,
            "kind": "emoji",
            "format": "mocha-language-pack-v1",
            "components": {
                "alias_table": "alias_table.json",
                "lexicon": "lexicon.jsonl",
                "frequency_table": "frequency.json",
                "context_model": "context_model.json",
            },
            "source": str(source),
            "entries": len(aliases),
        },
    )
    return BuildSummary(schema, len(aliases), str(out_dir), elapsed_ms(started), str(source))


def build_spanish_pack(args) -> BuildSummary | None:
    if not args.spanish_lexicon or not args.spanish_lexicon.exists():
        return None
    started = time.monotonic()
    entries = load_weighted_word_entries(args.spanish_lexicon, normalize_spanish_surface, args.max_entries)
    out_dir = args.language_pack_dir / "es-word"
    out_dir.mkdir(parents=True, exist_ok=True)

    reading_weights: Counter[str] = Counter()
    readings = []
    with (out_dir / "lexicon.jsonl").open("w", encoding="utf-8", newline="\n") as handle:
        for rank, (surface, source_weight) in enumerate(entries, start=1):
            reading = fold_spanish_word(surface)
            if len(reading) < 2:
                continue
            weight = source_weight if source_weight is not None else 1.0 / rank
            reading_weights[reading] += weight
            readings.append(reading)
            handle.write(
                json.dumps(
                    {"text": surface, "reading": reading, "compact": reading, "weight": weight},
                    ensure_ascii=False,
                    separators=(",", ":"),
                )
                + "\n"
            )

    write_json(
        out_dir / "schema_fst.json",
        {
            "format": "mocha-schema-fst-v1",
            "schema": "es-word",
            "consumes": "latin symbols",
            "emits": "spanish spelling lattice",
            "alphabet": "abcdefghijklmnopqrstuvwxyzñáéíóúü",
            "accent_restoration": True,
            "restores": ["á", "é", "í", "ó", "ú", "ü", "ñ"],
        },
    )
    write_frequency_table(out_dir, "es-word", reading_weights)
    write_context_model(out_dir, "es-word", reading_weights, "word")
    write_manifest(
        out_dir,
        {
            "schema": "es-word",
            "format": "mocha-language-pack-v1",
            "components": {
                "schema_fst": "schema_fst.json",
                "lexicon": "lexicon.jsonl",
                "frequency_table": "frequency.json",
                "context_model": "context_model.json",
            },
            "source": str(args.spanish_lexicon),
            "entries": len(readings),
        },
    )
    return BuildSummary("es-word", len(readings), str(out_dir), elapsed_ms(started), str(args.spanish_lexicon))


def build_russian_cyrillic_pack(args) -> BuildSummary | None:
    if not args.russian_lexicon or not args.russian_lexicon.exists():
        return None
    started = time.monotonic()
    entries = load_weighted_word_entries(args.russian_lexicon, normalize_cyrillic_surface, args.max_entries)
    out_dir = args.language_pack_dir / "ru-cyrillic"
    out_dir.mkdir(parents=True, exist_ok=True)

    word_weights: Counter[str] = Counter()
    surfaces = []
    with (out_dir / "lexicon.jsonl").open("w", encoding="utf-8", newline="\n") as handle:
        for rank, (surface, source_weight) in enumerate(entries, start=1):
            if len(surface) < 2:
                continue
            weight = source_weight if source_weight is not None else 1.0 / rank
            word_weights[surface] += weight
            surfaces.append(surface)
            handle.write(
                json.dumps(
                    {"text": surface, "reading": surface, "compact": surface, "weight": weight},
                    ensure_ascii=False,
                    separators=(",", ":"),
                )
                + "\n"
            )

    write_json(
        out_dir / "schema_fst.json",
        {
            "format": "mocha-schema-fst-v1",
            "schema": "ru-cyrillic",
            "consumes": "cyrillic symbols",
            "emits": "russian surface-form lattice",
            "alphabet": "абвгдеёжзийклмнопрстуфхцчшщъыьэюя",
        },
    )
    write_frequency_table(out_dir, "ru-cyrillic", word_weights)
    write_context_model(out_dir, "ru-cyrillic", word_weights, "word")
    write_morphology_surface_inventory(out_dir, "ru-cyrillic", word_weights, str(args.russian_lexicon))
    write_manifest(
        out_dir,
        {
            "schema": "ru-cyrillic",
            "format": "mocha-language-pack-v1",
            "components": {
                "schema_fst": "schema_fst.json",
                "lexicon": "lexicon.jsonl",
                "frequency_table": "frequency.json",
                "context_model": "context_model.json",
                "morphology": "morphology.json",
            },
            "source": str(args.russian_lexicon),
            "entries": len(surfaces),
        },
    )
    return BuildSummary("ru-cyrillic", len(surfaces), str(out_dir), elapsed_ms(started), str(args.russian_lexicon))


def build_russian_translit_pack(args) -> BuildSummary | None:
    if not args.russian_lexicon or not args.russian_lexicon.exists():
        return None
    started = time.monotonic()
    entries = load_weighted_word_entries(args.russian_lexicon, normalize_cyrillic_surface, args.max_entries)
    out_dir = args.language_pack_dir / "ru-translit"
    out_dir.mkdir(parents=True, exist_ok=True)

    reading_weights: Counter[str] = Counter()
    readings = []
    with (out_dir / "lexicon.jsonl").open("w", encoding="utf-8", newline="\n") as handle:
        for rank, (surface, source_weight) in enumerate(entries, start=1):
            reading = cyrillic_to_latin_translit(surface)
            if len(surface) < 2 or len(reading) < 2:
                continue
            weight = source_weight if source_weight is not None else 1.0 / rank
            reading_weights[reading] += weight
            readings.append(reading)
            handle.write(
                json.dumps(
                    {"text": surface, "reading": reading, "compact": reading, "weight": weight},
                    ensure_ascii=False,
                    separators=(",", ":"),
                )
                + "\n"
            )

    write_frequency_table(out_dir, "ru-translit", reading_weights)
    write_context_model(out_dir, "ru-translit", reading_weights, "word")
    write_transliteration_table(out_dir)
    write_manifest(
        out_dir,
        {
            "schema": "ru-translit",
            "format": "mocha-language-pack-v1",
            "components": {
                "lexicon": "lexicon.jsonl",
                "frequency_table": "frequency.json",
                "context_model": "context_model.json",
                "transliteration_table": "transliteration_table.json",
            },
            "source": str(args.russian_lexicon),
            "entries": len(readings),
        },
    )
    return BuildSummary("ru-translit", len(readings), str(out_dir), elapsed_ms(started), str(args.russian_lexicon))


def build_observation_error_model(args) -> BuildSummary:
    started = time.monotonic()
    out_dir = args.observation_model_dir / "geometry-phone-10col" / "qwerty"
    out_dir.mkdir(parents=True, exist_ok=True)
    stats = collect_futo_observation_stats(args.futo_jsonl, args.futo_rows)
    write_json(
        out_dir / "error_model.json",
        {
            "format": "mocha-observation-error-model-v1",
            "geometry": "geometry-phone-10col",
            "keymap_reference": "latin-qwerty",
            "source": str(args.futo_jsonl),
            "samples": stats["samples"],
            "features": {
                "points_per_trace": summarize(stats["point_counts"]),
                "step_length": summarize(stats["step_lengths"]),
                "path_length": summarize(stats["path_lengths"]),
                "startpoint_error": summarize(stats["startpoint_errors"]),
                "endpoint_error": summarize(stats["endpoint_errors"]),
            },
            "slot_errors": summarize_slot_errors(stats["slot_errors"]),
            "use_in_energy": "E_obs(O, q, G)",
        },
    )
    write_gesture_templates(out_dir, stats["gesture_templates"], args.max_gesture_templates)
    write_manifest(
        out_dir,
        {
            "schema": "observation",
            "format": "mocha-observation-pack-v1",
            "components": {
                "error_model": "error_model.json",
                "gesture_templates": "gesture_templates.json",
            },
            "source": str(args.futo_jsonl),
            "entries": stats["samples"],
        },
    )
    return BuildSummary(
        "observation-error-model",
        stats["samples"],
        str(out_dir),
        elapsed_ms(started),
        str(args.futo_jsonl),
    )


def load_rime_entries(root: Path, dictionary_name: str, limit: int | None) -> list[dict]:
    visited: set[Path] = set()
    out: list[dict] = []
    load_rime_table(root, dictionary_name, visited, out)
    out.sort(key=lambda entry: entry["weight"], reverse=True)
    return out[:limit] if limit else out


def load_rime_table(root: Path, name: str, visited: set[Path], out: list[dict]) -> None:
    path = root / f"{name}.dict.yaml"
    if not path.exists():
        path = root / name
    path = path.resolve()
    if path in visited or not path.exists():
        return
    visited.add(path)
    header, body = split_rime_yaml(path.read_text(encoding="utf-8", errors="ignore"))
    for imported in header.get("import_tables", []) or []:
        load_rime_table(root, str(imported), visited, out)
    for line in body.splitlines():
        if not line or line.startswith("#"):
            continue
        parts = line.split("\t")
        if len(parts) < 2:
            continue
        word, code = parts[0].strip(), parts[1].strip()
        if not word or not PINYIN_RE.match(code):
            continue
        weight = 1.0
        if len(parts) >= 3:
            try:
                weight = max(1.0, float(parts[2]))
            except ValueError:
                pass
        if normalize_pinyin(code).replace(" ", "").isascii():
            out.append({"text": word, "code": code, "weight": weight})


def split_rime_yaml(text: str) -> tuple[dict, str]:
    lines = text.splitlines()
    starts = [idx for idx, line in enumerate(lines) if line.strip() == "---"]
    ends = [idx for idx, line in enumerate(lines) if line.strip() == "..."]
    if starts and ends and ends[0] > starts[0]:
        header_text = "\n".join(lines[starts[0] + 1 : ends[0]])
        body = "\n".join(lines[ends[0] + 1 :])
        try:
            return yaml.safe_load(header_text) or {}, body
        except yaml.YAMLError:
            return {}, body
    return {}, text


def load_english_frequency_list(path: Path, limit: int | None) -> dict[str, float]:
    """Read `word<TAB>count` and log-compress counts into lexicon weights.

    Raw counts give entry_cost too wide a spread (a higher-frequency neighbour
    overrides geometry, e.g. your→you); log keeps frequency a gentle prior so the
    observation model decides among plausible words. ×1000 keeps f32 resolution.
    Returns an insertion-ordered dict (sorted by count desc in the file).
    """
    weights: dict[str, float] = {}
    with path.open("r", encoding="utf-8", errors="ignore") as handle:
        for line in handle:
            parts = line.rstrip("\n").split("\t")
            if len(parts) != 2:
                continue
            word = normalize_english_word(parts[0])
            try:
                count = float(parts[1])
            except ValueError:
                continue
            if len(word) < 1 or count <= 0 or word in weights:
                continue
            weights[word] = math.log1p(count) * 1000.0
            if limit and len(weights) >= limit:
                break
    return weights


def load_english_entries(path: Path, limit: int | None) -> list[str]:
    entries = []
    seen = set()
    with path.open("r", encoding="utf-8", errors="ignore") as handle:
        for line in handle:
            word = normalize_english_word(line.strip().split()[0] if line.strip() else "")
            if len(word) < 2 or word in seen:
                continue
            seen.add(word)
            entries.append(word)
            if limit and len(entries) >= limit:
                break
    return entries


def load_emoji_aliases(path: Path, language: str) -> list[tuple[str, list[str]]]:
    aliases = []
    seen = set()
    with path.open("r", encoding="utf-8", errors="ignore") as handle:
        for line in handle:
            parts = [part.strip() for part in line.rstrip("\n").split("\t") if part.strip()]
            if len(parts) < 2:
                continue
            alias = parts[0].lower()
            if not emoji_alias_matches_language(alias, language):
                continue
            outputs = []
            for output in parts[1:]:
                if output not in outputs:
                    outputs.append(output)
            if not alias or not outputs or alias in seen:
                continue
            seen.add(alias)
            aliases.append((alias, outputs))
    return aliases


def emoji_alias_matches_language(alias: str, language: str) -> bool:
    if language == "zh-hans":
        return any(is_cjk_char(ch) for ch in alias)
    if language == "en":
        return bool(re.fullmatch(r"[a-z0-9][a-z0-9_+.-]*", alias))
    raise ValueError(f"unsupported emoji language: {language}")


def is_cjk_char(ch: str) -> bool:
    return "\u4e00" <= ch <= "\u9fff"


def load_weighted_word_entries(path: Path, normalizer, limit: int | None) -> list[tuple[str, float | None]]:
    entries = []
    seen = set()
    with path.open("r", encoding="utf-8", errors="ignore") as handle:
        for line in handle:
            parsed = parse_weighted_word_line(line)
            if parsed is None:
                continue
            word, weight = parsed
            normalized = normalizer(word)
            if not normalized or normalized in seen:
                continue
            seen.add(normalized)
            entries.append((normalized, weight))
            if limit and len(entries) >= limit:
                break
    return entries


def parse_weighted_word_line(line: str) -> tuple[str, float | None] | None:
    text = line.strip()
    if not text or text.startswith("#"):
        return None
    if text.startswith("{"):
        try:
            record = json.loads(text)
        except json.JSONDecodeError:
            return None
        word = str(record.get("word") or record.get("text") or record.get("surface") or "").strip()
        weight = record.get("weight", record.get("count", record.get("frequency")))
        try:
            parsed_weight = float(weight) if weight is not None else None
        except (TypeError, ValueError):
            parsed_weight = None
        return (word, parsed_weight) if word else None

    parts = text.split("\t") if "\t" in text else text.split()
    if not parts:
        return None
    word = parts[0]
    weight = None
    if len(parts) > 1:
        weight = parse_float(parts[1])
        if weight is None and parse_float(parts[0]) is not None:
            weight = parse_float(parts[0])
            word = parts[1]
    return (word, weight)


def parse_float(value: str) -> float | None:
    try:
        return float(value)
    except (TypeError, ValueError):
        return None


def normalize_pinyin(value: str) -> str:
    value = value.strip().lower().replace("u:", "v").replace("ü", "v")
    decomposed = unicodedata.normalize("NFD", value)
    stripped = "".join(ch for ch in decomposed if unicodedata.category(ch) != "Mn")
    return unicodedata.normalize("NFC", stripped).replace("ü", "v")


def normalize_pinyin_segments(value: str) -> list[str]:
    return [
        "".join(ch for ch in normalize_pinyin(part) if ch.isascii() and ch.isalpha())
        for part in value.split()
        if part.strip()
    ]


def normalize_english_word(value: str) -> str:
    return "".join(ch.lower() for ch in value if ch.isascii() and ch.isalpha())


def normalize_spanish_surface(value: str) -> str:
    normalized = unicodedata.normalize("NFC", value.strip().lower())
    return "".join(
        ch
        for ch in normalized
        if ch.isascii() and ch.isalpha() or ch in "áéíóúüñ"
    )


def fold_spanish_word(value: str) -> str:
    replacements = str.maketrans({"á": "a", "é": "e", "í": "i", "ó": "o", "ú": "u", "ü": "u", "ñ": "n"})
    return normalize_spanish_surface(value).translate(replacements)


def normalize_cyrillic_surface(value: str) -> str:
    normalized = unicodedata.normalize("NFC", value.strip().lower())
    return "".join(ch for ch in normalized if "а" <= ch <= "я" or ch == "ё")


def cyrillic_to_latin_translit(value: str) -> str:
    rules = {
        "а": "a",
        "б": "b",
        "в": "v",
        "г": "g",
        "д": "d",
        "е": "e",
        "ё": "yo",
        "ж": "zh",
        "з": "z",
        "и": "i",
        "й": "j",
        "к": "k",
        "л": "l",
        "м": "m",
        "н": "n",
        "о": "o",
        "п": "p",
        "р": "r",
        "с": "s",
        "т": "t",
        "у": "u",
        "ф": "f",
        "х": "kh",
        "ц": "ts",
        "ч": "ch",
        "ш": "sh",
        "щ": "shch",
        "ъ": "",
        "ы": "y",
        "ь": "",
        "э": "e",
        "ю": "yu",
        "я": "ya",
    }
    return "".join(rules.get(ch, "") for ch in value)


def build_trie(values: list[str]) -> dict:
    root: dict = {}
    for value in values:
        node = root
        for ch in value:
            node = node.setdefault(ch, {})
        node["$"] = True
    return root


def build_pinyin_fuzzy_edges(syllables: list[str]) -> list[dict]:
    syllable_set = set(syllables)
    pairs = [("an", "ang"), ("en", "eng"), ("in", "ing"), ("l", "n"), ("f", "h"), ("zh", "z"), ("ch", "c"), ("sh", "s")]
    edges = []
    for syllable in syllables:
        for a, b in pairs:
            if syllable.startswith(a):
                alt = b + syllable[len(a) :]
                if alt in syllable_set:
                    edges.append({"from": syllable, "to": alt, "kind": "fuzzy", "cost": 1.0})
            if syllable.endswith(a):
                alt = syllable[: -len(a)] + b
                if alt in syllable_set:
                    edges.append({"from": syllable, "to": alt, "kind": "fuzzy", "cost": 1.0})
    return edges


def write_frequency_table(out_dir: Path, schema: str, weights: Counter[str]) -> None:
    total = float(sum(weights.values()))
    write_json(
        out_dir / "frequency.json",
        {
            "format": "mocha-frequency-table-v1",
            "schema": schema,
            "total_weight": total,
            "entries": len(weights),
            "top": [
                {"reading": reading, "weight": weight, "prob": weight / total if total else 0.0}
                for reading, weight in weights.most_common(2000)
            ],
        },
    )


def build_zh_lexicon_context_continuations(
    entries: list[tuple[str, list[str], float]],
    limit: int,
) -> list[dict]:
    counts: Counter[tuple[str, str, str]] = Counter()
    suffix_totals: Counter[str] = Counter()
    for text, segments, weight in entries:
        chars = list(text)
        if len(chars) < 2 or len(chars) > 6 or len(chars) != len(segments):
            continue
        for split in range(1, len(chars)):
            suffix = "".join(chars[:split])
            continuation_text = "".join(chars[split:])
            continuation_reading = " ".join(segments[split:])
            if not suffix or not continuation_text or not continuation_reading:
                continue
            counts[(suffix, continuation_reading, continuation_text)] += weight
            suffix_totals[suffix] += weight
    return context_continuation_records(counts, suffix_totals, limit)


def build_english_context_continuations(
    corpus_paths: list[Path],
    limit: int,
    max_lines: int | None,
) -> list[dict]:
    if not corpus_paths:
        return []
    counts: Counter[tuple[str, str, str]] = Counter()
    suffix_totals: Counter[str] = Counter()
    lines_seen = 0
    for text in iter_context_text(corpus_paths):
        tokens = english_context_tokens(text)
        readings = [reading for reading, _ in tokens]
        surfaces = [surface for _, surface in tokens]
        for idx in range(1, len(readings)):
            next_reading = readings[idx]
            next_surface = surfaces[idx]
            for width in (1, 2):
                start = idx - width
                if start < 0:
                    continue
                suffix = " ".join(readings[start:idx])
                counts[(suffix, next_reading, next_surface)] += 1.0
                suffix_totals[suffix] += 1.0
        lines_seen += 1
        if max_lines is not None and lines_seen >= max_lines:
            break
    return context_continuation_records(counts, suffix_totals, limit)


def collect_english_corpus_word_weights(
    corpus_paths: list[Path],
    max_lines: int | None,
) -> Counter[str]:
    weights: Counter[str] = Counter()
    if not corpus_paths:
        return weights
    lines_seen = 0
    for text in iter_context_text(corpus_paths):
        for reading, _ in english_context_tokens(text):
            weights[reading] += 1.0
        lines_seen += 1
        if max_lines is not None and lines_seen >= max_lines:
            break
    return weights


def collect_english_corpus_surface_weights(
    corpus_paths: list[Path],
    max_lines: int | None,
) -> Counter[tuple[str, str]]:
    weights: Counter[tuple[str, str]] = Counter()
    if not corpus_paths:
        return weights
    lines_seen = 0
    for text in iter_context_text(corpus_paths):
        for reading, surface in english_context_tokens(text):
            weights[(reading, surface)] += 1.0
        lines_seen += 1
        if max_lines is not None and lines_seen >= max_lines:
            break
    return weights


def english_context_tokens(text: str) -> list[tuple[str, str]]:
    normalized = normalize_english_context_text(text)
    tokens = []
    for token in EN_CONTEXT_TOKEN_RE.findall(normalized):
        surface = token.lower().strip("'")
        reading = normalize_english_word(surface)
        if len(reading) >= 2:
            tokens.append((reading, surface))
    return tokens


def normalize_english_context_text(text: str) -> str:
    text = text.replace("’", "'").replace("‘", "'")
    return EN_APOSTROPHE_JOIN_RE.sub(
        lambda match: f"{match.group(1)}'{match.group(2)}",
        text,
    )


def build_zh_corpus_context_continuations(
    corpus_paths: list[Path],
    entries: list[tuple[str, list[str], float]],
    limit: int,
    max_lines: int | None,
) -> list[dict]:
    if not corpus_paths:
        return []
    lexicon = {
        text: (" ".join(segments), weight)
        for text, segments, weight in entries
        if 1 <= len(text) <= 4 and len(text) == len(segments)
    }
    counts: Counter[tuple[str, str, str]] = Counter()
    suffix_totals: Counter[str] = Counter()
    lines_seen = 0
    for text in iter_context_text(corpus_paths):
        chars = [ch for ch in text if "\u4e00" <= ch <= "\u9fff"]
        for boundary in range(1, len(chars)):
            for suffix_width in range(1, 5):
                suffix_start = boundary - suffix_width
                if suffix_start < 0:
                    continue
                suffix = "".join(chars[suffix_start:boundary])
                if suffix not in lexicon:
                    continue
                for next_width in range(1, 5):
                    next_end = boundary + next_width
                    if next_end > len(chars):
                        continue
                    continuation_text = "".join(chars[boundary:next_end])
                    reading_weight = lexicon.get(continuation_text)
                    if reading_weight is None:
                        continue
                    continuation_reading, weight = reading_weight
                    counts[(suffix, continuation_reading, continuation_text)] += weight
                    suffix_totals[suffix] += weight
        lines_seen += 1
        if max_lines is not None and lines_seen >= max_lines:
            break
    return context_continuation_records(counts, suffix_totals, limit)


def build_zh_corpus_phrase_entries(
    corpus_paths: list[Path],
    base_entries: list[dict],
    limit: int,
    min_count: int,
    weight_scale: float,
) -> list[dict]:
    if not corpus_paths or limit <= 0:
        return []

    existing_texts = {str(entry["text"]) for entry in base_entries}
    char_readings: dict[str, tuple[list[str], float]] = {}
    for entry in base_entries:
        text = str(entry["text"])
        if len(text) != 1 or not is_cjk_char(text):
            continue
        segments = normalize_pinyin_segments(str(entry["code"]))
        if len(segments) != 1:
            continue
        weight = float(entry["weight"])
        current = char_readings.get(text)
        if current is None or weight > current[1]:
            char_readings[text] = (segments, weight)

    counts: Counter[str] = Counter()
    for text in iter_context_text(corpus_paths):
        chars = [ch for ch in text if is_cjk_char(ch)]
        for width in (3, 4):
            if len(chars) < width:
                continue
            for window in zip(*(chars[offset:] for offset in range(width))):
                phrase = "".join(window)
                if phrase in existing_texts:
                    continue
                if all(ch in char_readings for ch in phrase):
                    counts[phrase] += 1

    entries = []
    for phrase, count in counts.most_common():
        if count < min_count:
            break
        segments = [char_readings[ch][0][0] for ch in phrase]
        entries.append(
            {
                "text": phrase,
                "code": " ".join(segments),
                "weight": max(1.0, float(count) * weight_scale),
            }
        )
        if len(entries) >= limit:
            break
    return entries


def iter_context_text(paths: list[Path]):
    for path in paths:
        opener = gzip.open if path.suffix == ".gz" else open
        with opener(path, "rt", encoding="utf-8", errors="ignore") as handle:
            for line in handle:
                text = line.strip()
                if not text:
                    continue
                if text.startswith("{"):
                    try:
                        record = json.loads(text)
                    except json.JSONDecodeError:
                        continue
                    values = [
                        record.get(key)
                        for key in ("text", "content", "article", "body", "title")
                        if record.get(key)
                    ]
                    text = "\n".join(str(value) for value in values)
                if text:
                    yield text


def context_continuation_records(
    counts: Counter[tuple[str, str, str]],
    suffix_totals: Counter[str],
    limit: int,
) -> list[dict]:
    records = []
    for (suffix, reading, text), weight in counts.most_common(limit):
        total = suffix_totals[suffix]
        records.append(
            {
                "suffix": suffix,
                "reading": reading,
                "text": text,
                "weight": weight,
                "prob": weight / total if total else 0.0,
            }
        )
    return records


def merge_context_continuation_records(records: list[dict], limit: int) -> list[dict]:
    counts: Counter[tuple[str, str, str]] = Counter()
    suffix_totals: Counter[str] = Counter()
    for record in records:
        key = (record["suffix"], record["reading"], record["text"])
        weight = float(record.get("weight", 0.0))
        if weight <= 0.0:
            continue
        counts[key] += weight
        suffix_totals[record["suffix"]] += weight
    return context_continuation_records(counts, suffix_totals, limit)


def write_context_model(
    out_dir: Path,
    schema: str,
    weights: Counter[str],
    unit: str,
    continuations: list[dict] | None = None,
) -> None:
    continuations = continuations or []
    write_json(
        out_dir / "context_model.json",
        {
            "format": "mocha-context-model-v1",
            "schema": schema,
            "unit": unit,
            "entries": len(continuations),
            "top": [],
            "continuations": continuations,
            "use_in_energy": "E_context(x, r, b, z, C)",
            "note": "Context continuations only; base frequency priors live in frequency.json and personal/session context remains an overlay.",
        },
    )


def write_morphology_surface_inventory(out_dir: Path, schema: str, weights: Counter[str], source: str) -> None:
    write_json(
        out_dir / "morphology.json",
        {
            "format": "mocha-morphology-table-v1",
            "schema": schema,
            "source": source,
            "entries": len(weights),
            "surface_forms": [
                {
                    "surface": surface,
                    "lemma": surface,
                    "tags": ["surface"],
                    "weight": weight,
                }
                for surface, weight in weights.most_common(2000)
            ],
            "use_in_energy": "E_schema(a, r, b, z)",
            "note": "Surface-form inventory derived from the supplied Russian lexicon; no neural Kry model definitions are retained.",
        },
    )


def write_transliteration_table(out_dir: Path) -> None:
    rules = [
        ("shch", "щ"),
        ("yo", "ё"),
        ("zh", "ж"),
        ("kh", "х"),
        ("ts", "ц"),
        ("ch", "ч"),
        ("sh", "ш"),
        ("yu", "ю"),
        ("ya", "я"),
        ("a", "а"),
        ("b", "б"),
        ("v", "в"),
        ("g", "г"),
        ("d", "д"),
        ("e", "е"),
        ("z", "з"),
        ("i", "и"),
        ("j", "й"),
        ("k", "к"),
        ("l", "л"),
        ("m", "м"),
        ("n", "н"),
        ("o", "о"),
        ("p", "п"),
        ("r", "р"),
        ("s", "с"),
        ("t", "т"),
        ("u", "у"),
        ("f", "ф"),
        ("h", "х"),
        ("c", "к"),
        ("y", "ы"),
    ]
    write_json(
        out_dir / "transliteration_table.json",
        {
            "format": "mocha-transliteration-table-v1",
            "schema": "ru-translit",
            "source": "architecture rule table",
            "rules": [{"from": source, "to": target, "cost": 0.0} for source, target in rules],
            "use_in_energy": "E_schema(a, r, b, z)",
        },
    )


def collect_futo_observation_stats(path: Path, max_rows: int) -> dict:
    stats = {
        "samples": 0,
        "point_counts": [],
        "step_lengths": [],
        "path_lengths": [],
        "startpoint_errors": [],
        "endpoint_errors": [],
        "slot_errors": defaultdict(lambda: {"symbol": "", "startpoint_errors": [], "endpoint_errors": []}),
        "gesture_templates": defaultdict(lambda: {"count": 0, "path_lengths": [], "point_sums": None}),
    }
    keys = qwerty_keys()
    with path.open("r", encoding="utf-8", errors="ignore") as handle:
        for row, line in enumerate(handle):
            if max_rows and row >= max_rows:
                break
            if not line.strip():
                continue
            try:
                record = json.loads(line)
            except json.JSONDecodeError:
                continue
            word = normalize_english_word(str(record.get("word") or ""))
            points = [
                (float(point.get("x", 0.0)), float(point.get("y", 0.0)))
                for point in record.get("data") or []
                if isinstance(point, dict)
            ]
            if len(word) < 2 or len(points) < 2:
                continue
            stats["samples"] += 1
            stats["point_counts"].append(len(points))
            steps = [distance(a, b) for a, b in zip(points, points[1:])]
            stats["step_lengths"].extend(steps)
            stats["path_lengths"].append(sum(steps))
            add_gesture_template_sample(stats["gesture_templates"][word], points)
            if word[0] in keys:
                slot_id, center = keys[word[0]]
                error = distance(points[0], center)
                stats["startpoint_errors"].append(error)
                slot_stats = stats["slot_errors"][slot_id]
                slot_stats["symbol"] = word[0]
                slot_stats["startpoint_errors"].append(error)
            if word[-1] in keys:
                slot_id, center = keys[word[-1]]
                error = distance(points[-1], center)
                stats["endpoint_errors"].append(error)
                slot_stats = stats["slot_errors"][slot_id]
                slot_stats["symbol"] = word[-1]
                slot_stats["endpoint_errors"].append(error)
    return stats


def add_gesture_template_sample(accumulator: dict, points: list[tuple[float, float]], template_points: int = 16) -> None:
    sampled = resample_polyline(points, template_points)
    if not sampled:
        return
    if accumulator["point_sums"] is None:
        accumulator["point_sums"] = [[0.0, 0.0] for _ in sampled]
    accumulator["count"] += 1
    accumulator["path_lengths"].append(polyline_length(points))
    for idx, (x, y) in enumerate(sampled):
        accumulator["point_sums"][idx][0] += x
        accumulator["point_sums"][idx][1] += y


def resample_polyline(points: list[tuple[float, float]], count: int) -> list[tuple[float, float]]:
    if not points or count <= 0:
        return []
    if len(points) == 1 or count == 1:
        return [points[0]] * count
    segments = [distance(a, b) for a, b in zip(points, points[1:])]
    total = sum(segments)
    if total <= 0.0:
        return [points[0]] * count
    targets = [total * idx / (count - 1) for idx in range(count)]
    sampled = []
    segment_idx = 0
    traversed = 0.0
    for target in targets:
        while segment_idx < len(segments) - 1 and traversed + segments[segment_idx] < target:
            traversed += segments[segment_idx]
            segment_idx += 1
        segment_len = segments[segment_idx]
        if segment_len <= 0.0:
            sampled.append(points[segment_idx])
            continue
        local = (target - traversed) / segment_len
        a, b = points[segment_idx], points[segment_idx + 1]
        sampled.append((a[0] + (b[0] - a[0]) * local, a[1] + (b[1] - a[1]) * local))
    return sampled


def polyline_length(points: list[tuple[float, float]]) -> float:
    return sum(distance(a, b) for a, b in zip(points, points[1:]))


def qwerty_keys() -> dict[str, tuple[str, tuple[float, float]]]:
    rows = [("qwertyuiop", 0.0, 30.0), ("asdfghjkl", 15.0, 70.0), ("zxcvbnm", 45.0, 110.0)]
    keys = {}
    for row, (letters, x_offset, y) in enumerate(rows):
        for idx, ch in enumerate(letters):
            keys[ch] = (f"r{row}c{idx}", ((x_offset + idx * 30.0 + 15.0) / 300.0, y / 120.0))
    return keys


def distance(a: tuple[float, float], b: tuple[float, float]) -> float:
    return math.hypot(a[0] - b[0], a[1] - b[1])


def summarize(values: list[float]) -> dict:
    if not values:
        return {"count": 0}
    return {
        "count": len(values),
        "mean": statistics.fmean(values),
        "stdev": statistics.pstdev(values) if len(values) > 1 else 0.0,
        "min": min(values),
        "max": max(values),
    }


def summarize_slot_errors(slot_errors: dict) -> dict:
    out = {}
    for slot_id, values in sorted(slot_errors.items()):
        record = {"symbol": values["symbol"]}
        if values["startpoint_errors"]:
            record["startpoint_error"] = summarize(values["startpoint_errors"])
        if values["endpoint_errors"]:
            record["endpoint_error"] = summarize(values["endpoint_errors"])
        out[slot_id] = record
    return out


def write_gesture_templates(out_dir: Path, templates: dict, limit: int) -> None:
    records = []
    for word, accumulator in templates.items():
        count = accumulator["count"]
        point_sums = accumulator["point_sums"]
        if count <= 0 or point_sums is None:
            continue
        records.append(
            {
                "word": word,
                "count": count,
                "mean_path_length": statistics.fmean(accumulator["path_lengths"]) if accumulator["path_lengths"] else 0.0,
                "points": [[x / count, y / count] for x, y in point_sums],
            }
        )
    records.sort(key=lambda record: (-record["count"], record["word"]))
    write_json(
        out_dir / "gesture_templates.json",
        {
            "format": "mocha-gesture-templates-v1",
            "geometry": "geometry-phone-10col",
            "keymap_reference": "latin-qwerty",
            "point_count": len(records[0]["points"]) if records else 0,
            "templates": records[:limit],
            "use_in_energy": "E_obs(O, q, G)",
        },
    )


def write_manifest(out_dir: Path, manifest: dict) -> None:
    write_json(out_dir / "manifest.json", {**manifest, "generated_by": "tools/train_language_packs.py", "generated_at_unix": int(time.time())})


def inspect(args) -> None:
    records = build_status(args)
    observation_manifest = args.observation_model_dir / "geometry-phone-10col" / "qwerty" / "manifest.json"
    for record in records:
        print(f"{record['schema']}: status={record['status']} profile={record['profile']} pack={record['language_pack']}" + (f" reason={record['reason']}" if record["reason"] else ""))
    if observation_manifest.exists():
        manifest = json.loads(observation_manifest.read_text(encoding="utf-8"))
        status = "ready" if observation_pack_ready(observation_manifest.parent, manifest) else "missing"
        print(f"observation: status={status} entries={manifest['entries']} pack={observation_manifest.parent}")
    write_status(args, records)


def build_status(args) -> list[dict]:
    records = []
    for item in SCHEMA_REGISTRY:
        data_sources, default_reason = data_sources_for_schema(args, item)
        pack_dir = args.language_pack_dir / item["language_pack"]
        manifest_path = pack_dir / "manifest.json"
        has_pack = manifest_path.exists()
        missing_components = []
        if has_pack:
            manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
            components = manifest.get("components", {})
            for name in item["required_components"]:
                rel = components.get(name)
                if not rel or not (pack_dir / rel).exists():
                    missing_components.append(name)
        status = "ready" if has_pack and not missing_components else "missing_training_data"
        reason = ""
        if missing_components:
            reason = "missing components: " + ",".join(missing_components)
        elif not has_pack:
            reason = default_reason or item.get("reason", "language pack not built")
        records.append({"schema": item["schema"], "profile": item["profile"], "language_pack": str(pack_dir), "required_components": item["required_components"], "status": status, "reason": reason, "data": data_sources, "has_pack": has_pack})
    return records


def data_sources_for_schema(args, item: dict) -> tuple[list[str], str]:
    schema = item["schema"]
    if schema == "es-word":
        if args.spanish_lexicon:
            if args.spanish_lexicon.exists():
                return [str(args.spanish_lexicon)], "language pack not built from supplied Spanish lexicon"
            return [str(args.spanish_lexicon)], "Spanish lexicon path does not exist"
        return [], "missing Spanish lexicon in provided local data"
    if schema in {"ru-cyrillic", "ru-translit"}:
        if args.russian_lexicon:
            if args.russian_lexicon.exists():
                return [str(args.russian_lexicon)], "language pack not built from supplied Russian lexicon"
            return [str(args.russian_lexicon)], "Russian lexicon path does not exist"
        return [], "missing Russian lexicon in provided local data"
    return list(item["data"]), item.get("reason", "")


def write_status(args, records: list[dict]) -> None:
    observation_root = args.observation_model_dir / "geometry-phone-10col" / "qwerty"
    observation_manifest = observation_root / "manifest.json"
    observation_ready = False
    if observation_manifest.exists():
        observation_ready = observation_pack_ready(
            observation_root,
            json.loads(observation_manifest.read_text(encoding="utf-8")),
        )
    write_json(
        WORKSPACE / "assets" / "artifact-registry.json",
        {
            "format": "mocha-architecture-artifact-registry-v1",
            "note": "Kry neural model definitions are not part of this registry.",
            "language_packs": records,
            "observation_models": [
                {
                    "name": "futo-swipe-qwerty-error-model",
                    "path": str(observation_root),
                    "status": "ready" if observation_ready else "missing",
                }
            ],
        },
    )


def observation_pack_ready(root: Path, manifest: dict) -> bool:
    components = manifest.get("components", {})
    for name in ("error_model", "gesture_templates"):
        rel = components.get(name)
        if not rel or not (root / rel).exists():
            return False
    return True


def compile_binary_assets(args) -> None:
    registry = WORKSPACE / "assets" / "artifact-registry.json"
    subprocess.run(
        [
            "cargo",
            "run",
            "-p",
            "data-core",
            "--example",
            "compile_binary_assets",
            "--",
            str(registry),
        ],
        cwd=WORKSPACE,
        check=True,
    )


def write_json(path: Path, value) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2), encoding="utf-8")


def elapsed_ms(started: float) -> int:
    return int((time.monotonic() - started) * 1000)


def parse_args():
    parser = argparse.ArgumentParser()
    parser.add_argument("command", choices=["build", "build-lexicons", "inspect", "status"])
    parser.add_argument("--rime-root", type=Path, default=DEFAULT_RIME)
    parser.add_argument("--rime-dict", default="wanxiang")
    parser.add_argument("--english-dict-root", type=Path, default=DEFAULT_ENGLISH)
    parser.add_argument("--english-dict-file", default="top_english_words_lower_100000.txt")
    # Clean frequency list (word<TAB>count). Used in preference to the raw wordlist
    # when present. Pass an empty/non-existent path to force the legacy wordlist.
    parser.add_argument("--english-frequency-list", type=Path, default=DEFAULT_EN_FREQ)
    parser.add_argument("--english-context-corpus", type=Path, action="append")
    parser.add_argument("--zh-context-corpus", type=Path, action="append")
    parser.add_argument("--zh-phrase-corpus", type=Path, action="append")
    parser.add_argument("--spanish-lexicon", type=Path)
    parser.add_argument("--russian-lexicon", type=Path)
    parser.add_argument("--futo-jsonl", type=Path, default=DEFAULT_FUTO)
    parser.add_argument("--language-pack-dir", type=Path, default=WORKSPACE / "assets" / "language-packs")
    parser.add_argument("--observation-model-dir", type=Path, default=WORKSPACE / "assets" / "observation-models")
    parser.add_argument("--max-entries", type=int, default=200000)
    parser.add_argument("--max-schema-words", type=int, default=50000)
    parser.add_argument("--max-ambiguous-readings", type=int, default=16)
    parser.add_argument("--max-context-continuations", type=int, default=50000)
    parser.add_argument("--context-corpus-lines", type=int)
    parser.add_argument("--min-corpus-lexicon-count", type=int, default=2)
    parser.add_argument("--max-zh-corpus-phrases", type=int, default=0)
    parser.add_argument("--min-zh-corpus-phrase-count", type=int, default=5)
    parser.add_argument("--zh-corpus-phrase-weight-scale", type=float, default=1000.0)
    parser.add_argument("--skip-binary-assets", action="store_true")
    parser.add_argument("--futo-rows", type=int, default=200000)
    parser.add_argument("--max-gesture-templates", type=int, default=10000)
    return parser.parse_args()


if __name__ == "__main__":
    main()
