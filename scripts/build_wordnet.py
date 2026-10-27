"""把 Princeton WordNet 3.1 的 dict/ 压成 Lens 内置用的一份小文件（src-tauri/data/wordnet.tsv.gz）。

用法：python -I scripts/build_wordnet.py <WordNet 的 dict 目录>
下载：https://wordnetcode.princeton.edu/wn3.1.dict.tar.gz（解开后的 dict/）

只取 Lens 用得到的部分，每行一条、制表符分隔：
  S  pos  词|词|…            同义词集（第 i 行 S 的编号就是 i，从 0 开始）；pos = n v a r
  I  词  pos  编号,编号,…     这个词在这个词性下的义项，按 WordNet 的常用程度排序
  D  词  词                  派生关系（+ 派生、\\ 形容词 ↔ 副词、< 分词形容词），无向、去重
  E  pos  变形  原形          不规则变形（went → go）
词一律小写，短语里的空格写成 _。文件开头保留 WordNet 的许可声明（许可要求随数据分发）。
"""

import gzip
import os
import sys

POS_FILES = {"n": "noun", "v": "verb", "a": "adj", "r": "adv"}
DERIV_POINTERS = {"+", "\\", "<"}


def norm_pos(p):
    return "a" if p == "s" else p


def license_text(path):
    out = []
    with open(path, encoding="utf-8", errors="replace") as f:
        for line in f:
            if not line.startswith("  "):
                break
            out.append(line.strip().split(" ", 1)[1] if " " in line.strip() else "")
    return out


def main(src, dest):
    synsets = []  # (pos, [words])
    key = {}  # (file pos, offset) -> synset id
    raw_ptrs = []  # (synset id, source word index, pointer, target file pos, target offset, target word index)
    for p, name in POS_FILES.items():
        with open(os.path.join(src, f"data.{name}"), encoding="utf-8", errors="replace") as f:
            for line in f:
                if line.startswith("  "):
                    continue
                head = line.split(" | ")[0].split()
                offset, ss_type = head[0], head[2]
                w_cnt = int(head[3], 16)
                words = [head[4 + 2 * i].lower() for i in range(w_cnt)]
                # 形容词的 (a) (p) (ip) 位置标记去掉
                words = [w.split("(")[0] for w in words]
                sid = len(synsets)
                synsets.append((norm_pos(ss_type), words))
                key[(p, offset)] = sid
                k = 4 + 2 * w_cnt
                p_cnt = int(head[k])
                for j in range(p_cnt):
                    sym, toff, tpos, st = head[k + 1 + 4 * j: k + 5 + 4 * j]
                    if sym in DERIV_POINTERS:
                        raw_ptrs.append((sid, int(st[:2], 16), sym, norm_pos(tpos), toff, int(st[2:], 16)))

    def file_pos(p):
        return "a" if p in ("a", "s") else p

    deriv = set()
    for sid, sw, _sym, tpos, toff, tw in raw_ptrs:
        tid = key.get((file_pos(tpos), toff))
        if tid is None:
            continue
        src_words = synsets[sid][1]
        dst_words = synsets[tid][1]
        # 词汇级指针：源词、目标词各是第几个（从 1 开始）；0 表示整个同义词集
        a_list = src_words if sw == 0 else src_words[sw - 1: sw]
        b_list = dst_words if tw == 0 else dst_words[tw - 1: tw]
        for a in a_list:
            for b in b_list:
                if a != b:
                    deriv.add(tuple(sorted((a, b))))

    index_lines = []
    for p, name in POS_FILES.items():
        with open(os.path.join(src, f"index.{name}"), encoding="utf-8", errors="replace") as f:
            for line in f:
                if line.startswith("  "):
                    continue
                parts = line.split()
                lemma, synset_cnt, p_cnt = parts[0].lower(), int(parts[2]), int(parts[3])
                offsets = parts[6 + p_cnt: 6 + p_cnt + synset_cnt]
                ids = [key[(p, o)] for o in offsets if (p, o) in key]
                if ids:
                    index_lines.append(f"I\t{lemma}\t{p}\t{','.join(map(str, ids))}")

    exc_lines = []
    for p, name in POS_FILES.items():
        with open(os.path.join(src, f"{name}.exc"), encoding="utf-8", errors="replace") as f:
            for line in f:
                parts = line.split()
                if len(parts) >= 2 and parts[0] != parts[1]:
                    exc_lines.append(f"E\t{p}\t{parts[0].lower()}\t{parts[1].lower()}")

    lines = ["# " + l for l in license_text(os.path.join(src, "data.adv"))]
    lines += [f"S\t{p}\t{'|'.join(ws)}" for p, ws in synsets]
    lines += index_lines
    lines += [f"D\t{a}\t{b}" for a, b in sorted(deriv)]
    lines += exc_lines
    data = ("\n".join(lines) + "\n").encode("utf-8")
    with gzip.open(dest, "wb", compresslevel=9) as f:
        f.write(data)
    print(f"synsets {len(synsets)}, lemmas {len(index_lines)}, derivations {len(deriv)}, exceptions {len(exc_lines)}")
    print(f"raw {len(data) / 1e6:.1f} MB -> {os.path.getsize(dest) / 1e6:.2f} MB")


if __name__ == "__main__":
    here = os.path.dirname(os.path.abspath(__file__))
    main(sys.argv[1], os.path.join(here, "..", "src-tauri", "data", "wordnet.tsv.gz"))
