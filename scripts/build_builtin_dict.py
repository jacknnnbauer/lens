#!/usr/bin/env python3
"""生成内置英汉词典 src-tauri/data/lens-ecdict.zip（StarDict 格式）。

    py -3 -I scripts/build_builtin_dict.py <ecdict.csv> src-tauri/data/lens-ecdict.zip

词条来自 ECDICT（https://github.com/skywind3000/ECDICT，MIT 许可），词头取内置词频表的 4 万原形。
只读 COLUMNS 里的四列：ECDICT 的 frq 列是当代语料库词频，bnc 列是 BNC 词频，这些数据不能带进软件。
"""

import csv
import gzip
import html
import io
import re
import struct
import sys
import zipfile
from pathlib import Path

COLUMNS = ("word", "phonetic", "translation", "exchange")
BOOKNAME = "简明英汉（ECDICT）"
BASE = "lens-ecdict"
# 压缩后的上限（字节）：安装包增加不超过 8 MB，给题库和界面代码留出余量
MAX_ZIP = 7 * 1024 * 1024
# 每个词条最多保留的释义行数
MAX_LINES = 8
# exchange 里表示变形的类型：过去式、过去分词、现在分词、第三人称单数、比较级、最高级、复数
FORM_TYPES = set("pdi3rts")
FREQ = Path(__file__).resolve().parent.parent / "src-tauri" / "data" / "wordfreq_en.txt.gz"


def load_lemmas() -> list[str]:
    with gzip.open(FREQ, "rt", encoding="utf-8") as f:
        return [w for w in (line.strip() for line in f) if w and not w.startswith("#")]


def load_rows(path: str, wanted: set[str]) -> dict[str, dict[str, str]]:
    csv.field_size_limit(10_000_000)
    rows: dict[str, dict[str, str]] = {}
    with open(path, encoding="utf-8", newline="") as f:
        for row in csv.DictReader(f):
            word = row["word"]
            if word in wanted and row["translation"].strip():
                rows[word] = {k: row[k] for k in COLUMNS}
    return rows


def entry_html(row: dict[str, str]) -> str:
    lines = [ln.strip() for ln in row["translation"].split("\\n") if ln.strip()]
    parts = []
    if row["phonetic"].strip():
        parts.append(f'<div class="ph">/{html.escape(row["phonetic"].strip())}/</div>')
    parts.extend(f"<div>{html.escape(ln)}</div>" for ln in lines[:MAX_LINES])
    return "".join(parts)


def forms_of(exchange: str) -> list[str]:
    out = []
    for item in exchange.split("/"):
        kind, _, form = item.partition(":")
        if kind in FORM_TYPES and re.fullmatch(r"[A-Za-z][A-Za-z'-]*", form):
            out.append(form)
    return out


def sort_key(word: str) -> tuple[bytes, bytes]:
    raw = word.encode("utf-8")
    return (raw.lower(), raw)


def build(rows: dict[str, dict[str, str]], words: list[str]) -> bytes:
    words = sorted(words, key=sort_key)
    index_of = {w: i for i, w in enumerate(words)}
    body, idx = bytearray(), bytearray()
    for w in words:
        data = entry_html(rows[w]).encode("utf-8")
        idx += w.encode("utf-8") + b"\0" + struct.pack(">II", len(body), len(data))
        body += data
    syns: dict[str, int] = {}
    for w in words:
        for form in forms_of(rows[w]["exchange"]):
            # 变形本身也是词头（left、saw）时不抢它的位置
            if form not in index_of:
                syns.setdefault(form, index_of[w])
    syn = bytearray()
    for form in sorted(syns, key=sort_key):
        syn += form.encode("utf-8") + b"\0" + struct.pack(">I", syns[form])
    ifo = (
        "StarDict's dict ifo file\n"
        "version=2.4.2\n"
        f"wordcount={len(words)}\n"
        f"synwordcount={len(syns)}\n"
        f"idxfilesize={len(idx)}\n"
        f"bookname={BOOKNAME}\n"
        "sametypesequence=h\n"
        "description=Derived from ECDICT (MIT License).\n"
    )
    out = io.BytesIO()
    with zipfile.ZipFile(out, "w", zipfile.ZIP_DEFLATED, compresslevel=9) as z:
        for name, data in (("ifo", ifo.encode("utf-8")), ("idx", bytes(idx)), ("syn", bytes(syn)), ("dict", bytes(body))):
            info = zipfile.ZipInfo(f"{BASE}.{name}", date_time=(2026, 1, 1, 0, 0, 0))
            info.compress_type = zipfile.ZIP_DEFLATED
            z.writestr(info, data, compresslevel=9)
    print(f"词头 {len(words)}，变形 {len(syns)}，压缩后 {out.tell() / 1024 / 1024:.2f} MB")
    return out.getvalue()


def main() -> None:
    src, dest = sys.argv[1], sys.argv[2]
    lemmas = load_lemmas()
    rows = load_rows(src, set(lemmas))
    ranked = [w for w in lemmas if w in rows]
    while True:
        data = build(rows, ranked)
        if len(data) <= MAX_ZIP:
            break
        # 超了就去掉词频最低的一批，释义不缩减
        ranked = ranked[: int(len(ranked) * 0.95)]
    Path(dest).write_bytes(data)


if __name__ == "__main__":
    main()
