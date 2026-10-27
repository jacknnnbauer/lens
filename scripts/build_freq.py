"""把 wordfreq 的英文词频表（large_en.msgpack.gz）压成 Lens 内置的原形频率排名表。

用法：python scripts/build_freq.py <large_en.msgpack.gz>
  large_en.msgpack.gz 在 wordfreq 的 wheel 里（pip download wordfreq --no-deps，解压 wordfreq/data/）。
输出：src-tauri/data/wordfreq_en.txt.gz，每行一个原形，按频率从高到低（行号 = 排名）。
  词形用内置 WordNet（src-tauri/data/wordnet.tsv.gz）的不规则变形表和词尾规则归到原形，
  原形的排名取它最常见的那个词形。

数据许可：wordfreq 的数据按 CC BY-SA 4.0 分发（https://github.com/rspeer/wordfreq），
  生成的文件开头保留出处与许可说明。
"""

import gzip
import os
import re
import struct
import sys

TOP = 40000
WORD = re.compile(r"^[a-z]+(?:-[a-z]+)*$")


def unpack(buf, i=0):
    """最小的 msgpack 解码：只认 wordfreq 文件里会出现的类型"""
    b = buf[i]
    i += 1
    if b <= 0x7F:
        return b, i
    if 0x80 <= b <= 0x8F:
        return unpack_map(buf, i, b & 0x0F)
    if 0x90 <= b <= 0x9F:
        return unpack_array(buf, i, b & 0x0F)
    if 0xA0 <= b <= 0xBF:
        n = b & 0x1F
        return buf[i:i + n].decode("utf-8"), i + n
    if b == 0xC0:
        return None, i
    if b in (0xC2, 0xC3):
        return b == 0xC3, i
    if b == 0xCC:
        return buf[i], i + 1
    if b == 0xCD:
        return struct.unpack(">H", buf[i:i + 2])[0], i + 2
    if b == 0xCE:
        return struct.unpack(">I", buf[i:i + 4])[0], i + 4
    if b == 0xD9:
        n = buf[i]
        return buf[i + 1:i + 1 + n].decode("utf-8"), i + 1 + n
    if b == 0xDA:
        n = struct.unpack(">H", buf[i:i + 2])[0]
        return buf[i + 2:i + 2 + n].decode("utf-8"), i + 2 + n
    if b == 0xDC:
        return unpack_array(buf, i + 2, struct.unpack(">H", buf[i:i + 2])[0])
    if b == 0xDD:
        return unpack_array(buf, i + 4, struct.unpack(">I", buf[i:i + 4])[0])
    if b == 0xDE:
        return unpack_map(buf, i + 2, struct.unpack(">H", buf[i:i + 2])[0])
    raise ValueError(f"unsupported msgpack byte {b:#x} at {i - 1}")


def unpack_array(buf, i, n):
    out = []
    for _ in range(n):
        v, i = unpack(buf, i)
        out.append(v)
    return out, i


def unpack_map(buf, i, n):
    out = {}
    for _ in range(n):
        k, i = unpack(buf, i)
        v, i = unpack(buf, i)
        out[k] = v
    return out, i


def load_wordnet(path):
    lemmas, exc = set(), {}
    with gzip.open(path, "rt", encoding="utf-8") as f:
        for line in f:
            parts = line.rstrip("\n").split("\t")
            if parts[0] == "I":
                lemmas.add(parts[1])
            elif parts[0] == "E":
                exc.setdefault(parts[2], parts[3])
    return lemmas, exc


def candidates(w):
    """词尾规则：可能的原形（与 Rust 端 suffix_candidates 的思路一致）"""
    out = []
    for suf, rep in (("ies", "y"), ("es", ""), ("s", ""), ("ied", "y"), ("ed", ""), ("ed", "e"), ("ing", ""), ("ing", "e"),
                     ("ier", "y"), ("iest", "y"), ("er", ""), ("est", ""), ("er", "e"), ("est", "e")):
        if w.endswith(suf) and len(w) - len(suf) >= 2:
            base = w[: len(w) - len(suf)] + rep
            out.append(base)
            # 双写辅音：stopped → stop、bigger → big
            if rep == "" and len(base) >= 3 and base[-1] == base[-2] and base[-1] not in "aeiou":
                out.append(base[:-1])
    return out


def lemma_of(w, lemmas, exc):
    if w in exc:
        return exc[w]
    # WordNet 也把 rusted、suppressed、jeans 这类词形收成词：有更短的原形就归过去（physics、glass 例外）
    if w in lemmas and (w.endswith(("ics", "ss", "us")) or not w.endswith(("s", "ed", "ing"))):
        return w
    for c in candidates(w):
        if c in lemmas:
            return c
    return w


def main(src):
    here = os.path.dirname(os.path.abspath(__file__))
    data = os.path.join(here, "..", "src-tauri", "data")
    lemmas, exc = load_wordnet(os.path.join(data, "wordnet.tsv.gz"))
    with gzip.open(src, "rb") as f:
        packed, _ = unpack(f.read())
    header, buckets = packed[0], packed[1:]
    assert header.get("format") == "cB", header
    seen, ranked = set(), []
    for bucket in buckets:
        for w in bucket:
            if w.endswith("'s"):
                w = w[:-2]
            # 缩约形式（it's、they'd）不收
            if "'" in w or not WORD.match(w) or (len(w) == 1 and w not in ("a", "i")):
                continue
            lem = lemma_of(w, lemmas, exc)
            if lem not in seen:
                seen.add(lem)
                ranked.append(lem)
        if len(ranked) >= TOP:
            break
    ranked = ranked[:TOP]
    lines = [
        "# Word frequency ranks derived from wordfreq 3.1 (Robyn Speer, https://github.com/rspeer/wordfreq)",
        "# Data licensed under CC BY-SA 4.0 (https://creativecommons.org/licenses/by-sa/4.0/).",
        "# Inflected forms merged into lemmas by Lens using WordNet 3.1; one lemma per line, most frequent first.",
    ] + ranked
    dest = os.path.join(data, "wordfreq_en.txt.gz")
    with gzip.open(dest, "wb", compresslevel=9) as f:
        f.write(("\n".join(lines) + "\n").encode("utf-8"))
    print(f"lemmas {len(ranked)} -> {os.path.getsize(dest) / 1e3:.0f} KB")
    for w in ("the", "be", "house", "abandon", "ubiquitous", "rusty", "obstinate", "relinquish", "mitigate", "ephemeral"):
        print(w, ranked.index(w) + 1 if w in ranked else None)


if __name__ == "__main__":
    main(sys.argv[1])
