"""生成 StarDict 测试词典（自制内容，不含任何商业词典数据）。

    python fixtures/stardict/make_fixtures.py

按 StarDict 格式文档独立实现写入端，用来交叉验证 Rust 读取端：
- roots-tm/   2.4.2，32 位 offset，sametypesequence=tm（多字段），.idx + .dict + .syn
- roots-h-dz/ 3.0.0，idxoffsetbits=64，sametypesequence=h，.idx.gz + .dict.dz（64 字节小块，词条跨块）
- mixed/      无 sametypesequence，每个字段自带类型字符（含大写二进制字段 W）
"""

import gzip
import struct
import zlib
from pathlib import Path

HERE = Path(__file__).resolve().parent


def sort_key(word: str):
    # StarDict 要求 .idx 按 g_ascii_strcasecmp 再按 strcmp 排序
    b = word.encode("utf-8")
    return (b.lower(), b)


def dictzip(data: bytes, chunk_len: int) -> bytes:
    """dictzip：gzip + FEXTRA 'RA' 子字段记录每块压缩长度；块之间 Z_FULL_FLUSH，可独立解压。"""
    comp = zlib.compressobj(9, zlib.DEFLATED, -15)
    chunks = []
    for i in range(0, len(data), chunk_len):
        chunks.append(comp.compress(data[i : i + chunk_len]) + comp.flush(zlib.Z_FULL_FLUSH))
    chunks[-1] += comp.flush(zlib.Z_FINISH)

    ra = struct.pack("<HHH", 1, chunk_len, len(chunks)) + b"".join(struct.pack("<H", len(c)) for c in chunks)
    extra = b"RA" + struct.pack("<H", len(ra)) + ra
    header = (
        b"\x1f\x8b\x08"
        + bytes([0x04 | 0x08])  # FEXTRA | FNAME
        + b"\x00\x00\x00\x00\x02\x03"
        + struct.pack("<H", len(extra))
        + extra
        + b"fixture.dict\x00"
    )
    trailer = struct.pack("<II", zlib.crc32(data) & 0xFFFFFFFF, len(data) & 0xFFFFFFFF)
    return header + b"".join(chunks) + trailer


def write_dict(name, entries, *, version="2.4.2", sts=None, offset_bits=32, syn=(), gz_idx=False, dz_chunk=None):
    """entries: [(word, bytes)]；syn: [(同义词, 目标 word)]"""
    out = HERE / name
    out.mkdir(exist_ok=True)
    for f in out.iterdir():
        f.unlink()

    entries = sorted(entries, key=lambda e: sort_key(e[0]))
    data = b""
    idx = b""
    ordinal = {}
    for i, (word, body) in enumerate(entries):
        ordinal.setdefault(word, i)
        off_fmt = ">Q" if offset_bits == 64 else ">I"
        idx += word.encode("utf-8") + b"\x00" + struct.pack(off_fmt, len(data)) + struct.pack(">I", len(body))
        data += body

    ifo = [
        "StarDict's dict ifo file",
        f"version={version}",
        f"bookname={name}（测试）",
        f"wordcount={len(entries)}",
        f"idxfilesize={len(idx)}",
        "author=Lens fixtures",
        "description=自制测试词典",
    ]
    if offset_bits == 64:
        ifo.append("idxoffsetbits=64")
    if sts:
        ifo.append(f"sametypesequence={sts}")

    if syn:
        syn_bytes = b""
        for s, target in sorted(syn, key=lambda e: sort_key(e[0])):
            syn_bytes += s.encode("utf-8") + b"\x00" + struct.pack(">I", ordinal[target])
        (out / f"{name}.syn").write_bytes(syn_bytes)
        ifo.append(f"synwordcount={len(syn)}")

    (out / f"{name}.ifo").write_text("\n".join(ifo) + "\n", encoding="utf-8", newline="\n")
    if gz_idx:
        (out / f"{name}.idx.gz").write_bytes(gzip.compress(idx, mtime=0))
    else:
        (out / f"{name}.idx").write_bytes(idx)
    if dz_chunk:
        (out / f"{name}.dict.dz").write_bytes(dictzip(data, dz_chunk))
    else:
        (out / f"{name}.dict").write_bytes(data)


def tm(phonetic: str, text: str) -> bytes:
    # sametypesequence=tm：非末尾的小写字段以 \0 结尾，末尾字段占到词条结束
    return phonetic.encode("utf-8") + b"\x00" + text.encode("utf-8")


def typed(*fields) -> bytes:
    out = b""
    for t, payload in fields:
        if t.islower():
            out += t.encode() + payload + b"\x00"
        else:
            out += t.encode() + struct.pack(">I", len(payload)) + payload
    return out


write_dict(
    "roots-tm",
    [
        ("ubiquitous", tm("juːˈbɪkwɪtəs", "adj. 无处不在的\n← ubique 处处")),
        ("benevolent", tm("bəˈnevələnt", "adj. 仁慈的\n← bene 好 + vol 意愿")),
        ("bank", tm("bæŋk", "n. 河岸")),
        ("bank", tm("bæŋk", "n. 银行")),
        ("run", tm("rʌn", "v. 跑")),
        ("Polish", tm("ˈpəʊlɪʃ", "adj. 波兰的")),
        ("polish", tm("ˈpɒlɪʃ", "v. 擦亮")),
    ],
    sts="tm",
    syn=[("ubiquity", "ubiquitous"), ("ran", "run"), ("running", "run")],
)

long_html = "<p>" + "；".join(f"义项 {i}：在很多场合都会出现的" for i in range(1, 13)) + "</p>"
write_dict(
    "roots-h-dz",
    [
        ("ubiquitous", "<p><b>ubique</b> 处处 ← <i>ubi</i> 哪里</p>".encode("utf-8")),
        ("receive", "<p><b>re-</b> 回 + <b>ceive</b> 拿</p>".encode("utf-8")),
        ("omnipresent", long_html.encode("utf-8")),
        ("café", "<p>法语 <i>café</i> 咖啡（馆）</p>".encode("utf-8")),
    ],
    version="3.0.0",
    sts="h",
    offset_bits=64,
    gz_idx=True,
    dz_chunk=64,
)

write_dict(
    "mixed",
    [
        ("ubiquitous", typed(("t", "juːˈbɪkwɪtəs".encode()), ("m", "无处不在的".encode()))),
        ("bank", typed(("h", "<b>bank</b> 银行".encode()), ("m", "a & b < c".encode()))),
        ("polish", typed(("m", "擦亮".encode()), ("W", b"RIFF"))),
        ("sound", typed(("W", b"RIFF"),)),
    ],
)

print("fixtures written to", HERE)
