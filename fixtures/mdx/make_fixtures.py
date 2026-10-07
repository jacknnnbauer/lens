"""生成 mdx / mdd 测试词典（自制内容，不含任何商业词典数据）。

    python fixtures/mdx/make_fixtures.py

按 MDict 文件格式独立实现写入端（含 RIPEMD-128、key block info 加密、LZO 字面量编码），
用来交叉验证 Rust 读取端：
- basic/      2.0，UTF-8，zlib，Encrypted=2，StyleSheet，@@@LINK（含环），多 key block / record block
              + basic.mdd、basic.1.mdd（UTF-16 资源路径）
- legacy/     1.2，GBK，LZO，32 位整数，key block info 不压缩
- utf16/      2.0，UTF-16LE 编码，不压缩（type 0）
- regcode.mdx Encrypted=1（需要注册码），应被拒绝
- v3.mdx      GeneratedByEngineVersion=3.0，应被拒绝
"""

import struct
import zlib
from html import escape
from pathlib import Path

HERE = Path(__file__).resolve().parent

# ---------------------------------------------------------------- RIPEMD-128

_RL = [list(range(16)),
       [7, 4, 13, 1, 10, 6, 15, 3, 12, 0, 9, 5, 2, 14, 11, 8],
       [3, 10, 14, 4, 9, 15, 8, 1, 2, 7, 0, 6, 13, 11, 5, 12],
       [1, 9, 11, 10, 0, 8, 12, 4, 13, 3, 7, 15, 14, 5, 6, 2]]
_RR = [[5, 14, 7, 0, 9, 2, 11, 4, 13, 6, 15, 8, 1, 10, 3, 12],
       [6, 11, 3, 7, 0, 13, 5, 10, 14, 15, 8, 12, 4, 9, 1, 2],
       [15, 5, 1, 3, 7, 14, 6, 9, 11, 8, 12, 2, 10, 0, 4, 13],
       [8, 6, 4, 1, 3, 11, 15, 0, 5, 12, 2, 13, 9, 7, 10, 14]]
_SL = [[11, 14, 15, 12, 5, 8, 7, 9, 11, 13, 14, 15, 6, 7, 9, 8],
       [7, 6, 8, 13, 11, 9, 7, 15, 7, 12, 15, 9, 11, 7, 13, 12],
       [11, 13, 6, 7, 14, 9, 13, 15, 14, 8, 13, 6, 5, 12, 7, 5],
       [11, 12, 14, 15, 14, 15, 9, 8, 9, 14, 5, 6, 8, 6, 5, 12]]
_SR = [[8, 9, 9, 11, 13, 15, 15, 5, 7, 7, 8, 11, 14, 14, 12, 6],
       [9, 13, 15, 7, 12, 8, 9, 11, 7, 7, 12, 7, 6, 15, 13, 11],
       [9, 7, 15, 11, 8, 6, 6, 14, 12, 13, 5, 14, 13, 13, 7, 5],
       [15, 5, 8, 11, 14, 14, 6, 14, 6, 9, 12, 9, 12, 5, 15, 8]]
_KL = [0x00000000, 0x5A827999, 0x6ED9EBA1, 0x8F1BBCDC]
_KR = [0x50A28BE6, 0x5C4DD124, 0x6D703EF3, 0x00000000]
_M = 0xFFFFFFFF


def _f(j, x, y, z):
    return [x ^ y ^ z, (x & y) | (~x & z), (x | ~y) ^ z, (x & z) | (y & ~z)][j] & _M


def _rol(x, n):
    return ((x << n) | (x >> (32 - n))) & _M


def ripemd128(msg: bytes) -> bytes:
    h = [0x67452301, 0xEFCDAB89, 0x98BADCFE, 0x10325476]
    padded = msg + b"\x80" + b"\x00" * ((55 - len(msg)) % 64) + struct.pack("<Q", len(msg) * 8)
    for off in range(0, len(padded), 64):
        x = struct.unpack("<16I", padded[off : off + 64])
        al, bl, cl, dl = h
        ar, br, cr, dr = h
        for rnd in range(4):
            for i in range(16):
                t = _rol((al + _f(rnd, bl, cl, dl) + x[_RL[rnd][i]] + _KL[rnd]) & _M, _SL[rnd][i])
                al, dl, cl, bl = dl, cl, bl, t
                t = _rol((ar + _f(3 - rnd, br, cr, dr) + x[_RR[rnd][i]] + _KR[rnd]) & _M, _SR[rnd][i])
                ar, dr, cr, br = dr, cr, br, t
        t = (h[1] + cl + dr) & _M
        h[1] = (h[2] + dl + ar) & _M
        h[2] = (h[3] + al + br) & _M
        h[3] = (h[0] + bl + cr) & _M
        h[0] = t
    return struct.pack("<4I", *h)


assert ripemd128(b"").hex() == "cdf26213a150dc3ecb610f18f6b38b46"
assert ripemd128(b"abc").hex() == "c14a12199c66e4ba84636b0f69144c77"
assert ripemd128(b"a" * 1000).hex() == ripemd128(b"a" * 1000).hex()


def fast_encrypt(data: bytes, key: bytes) -> bytes:
    """MDict 解密的逆运算：c = swap_nibbles(p ^ prev ^ i ^ key[i])，prev = c。"""
    out = bytearray()
    prev = 0x36
    for i, p in enumerate(data):
        t = p ^ prev ^ (i & 0xFF) ^ key[i % len(key)]
        c = ((t >> 4) | (t << 4)) & 0xFF
        out.append(c)
        prev = c
    return bytes(out)


# ---------------------------------------------------------------- 压缩

def lzo_literal(data: bytes) -> bytes:
    """只含一段字面量的合法 LZO1X 流：长度指令 + 原文 + EOS（0x11 0x00 0x00）。"""
    n = len(data)
    assert n >= 4
    t = n - 3
    if t <= 15:
        head = bytes([t])
    else:
        rem = t - 15
        head = bytearray([0])
        while rem > 255:
            head.append(0)
            rem -= 255
        head.append(rem)
        head = bytes(head)
    return head + data + b"\x11\x00\x00"


def block(raw: bytes, ctype: int) -> bytes:
    payload = {0: lambda d: d, 1: lzo_literal, 2: zlib.compress}[ctype](raw)
    return struct.pack("<I", ctype) + struct.pack(">I", zlib.adler32(raw)) + payload


# ---------------------------------------------------------------- 写 mdx / mdd

def write_mdx(path, entries, *, version="2.0", encoding="UTF-8", encrypted=0, ctype=2,
              keys_per_block=3, record_block=64, stylesheet=None, title="", is_mdd=False):
    """entries: [(key, record)]，record 为 str（mdx）或 bytes（mdd）。"""
    v2 = float(version) >= 2.0
    num = ">Q" if v2 else ">I"
    size_fmt = ">H" if v2 else ">B"
    term = 1 if v2 else 0
    codec = {"UTF-8": "utf-8", "GBK": "gbk", "UTF-16": "utf-16-le"}[encoding] if not is_mdd else "utf-16-le"
    unit = 2 if codec == "utf-16-le" else 1
    nul = b"\x00" * unit

    # 记录：mdx 正文以 \r\n\0 结尾；mdd 为原始字节
    entries = sorted(entries, key=lambda e: (e[0].lower(), e[0]))
    records, key_ids = b"", []
    for _, rec in entries:
        key_ids.append(len(records))
        records += rec if is_mdd else (rec + "\r\n").encode(codec) + nul

    # key blocks + key block info
    key_blocks, info = b"", b""
    for b in range(0, len(entries), keys_per_block):
        chunk = list(range(b, min(b + keys_per_block, len(entries))))
        raw = b"".join(struct.pack(num, key_ids[i]) + entries[i][0].encode(codec) + nul for i in chunk)
        blk = block(raw, ctype)
        key_blocks += blk
        first, last = entries[chunk[0]][0].encode(codec), entries[chunk[-1]][0].encode(codec)
        info += struct.pack(num, len(chunk))
        for k in (first, last):
            info += struct.pack(size_fmt, len(k) // unit) + k + nul * term
        info += struct.pack(num, len(blk)) + struct.pack(num, len(raw))
    num_key_blocks = (len(entries) + keys_per_block - 1) // keys_per_block

    if v2:
        info_block = b"\x02\x00\x00\x00" + struct.pack(">I", zlib.adler32(info)) + zlib.compress(info)
        if encrypted & 2:
            key = ripemd128(info_block[4:8] + struct.pack("<I", 0x3695))
            info_block = info_block[:8] + fast_encrypt(info_block[8:], key)
        kw = struct.pack(">QQQQQ", num_key_blocks, len(entries), len(info), len(info_block), len(key_blocks))
        kw += struct.pack(">I", zlib.adler32(kw))
    else:
        info_block = info
        kw = struct.pack(">IIII", num_key_blocks, len(entries), len(info_block), len(key_blocks))

    # record blocks：按记录边界切块，不把一条记录拆到两块
    bounds = key_ids + [len(records)]
    rec_blocks, rec_info, cur = b"", b"", b""
    for i in range(len(entries)):
        r = records[bounds[i] : bounds[i + 1]]
        if cur and len(cur) + len(r) > record_block:
            blk = block(cur, ctype)
            rec_blocks += blk
            rec_info += struct.pack(num, len(blk)) + struct.pack(num, len(cur))
            cur = b""
        cur += r
    blk = block(cur, ctype)
    rec_blocks += blk
    rec_info += struct.pack(num, len(blk)) + struct.pack(num, len(cur))
    n_rec = len(rec_info) // (16 if v2 else 8)
    fmt4 = ">QQQQ" if v2 else ">IIII"
    rec = struct.pack(fmt4, n_rec, len(entries), len(rec_info), len(rec_blocks)) + rec_info + rec_blocks

    tag = "Library_Data" if is_mdd else "Dictionary"
    attrs = {
        "GeneratedByEngineVersion": version,
        "RequiredEngineVersion": version,
        "Encrypted": str(encrypted) if encrypted else "No",
        "Encoding": "" if is_mdd else encoding,
        "Format": "" if is_mdd else "Html",
        "KeyCaseSensitive": "No",
        "StripKey": "Yes",
        "Title": title,
        "Description": "<p>自制测试词典 & 示例</p>",
        "StyleSheet": stylesheet or "",
    }
    xml = f"<{tag} " + " ".join(f'{k}="{escape(v, quote=True)}"' for k, v in attrs.items()) + "/>\r\n"
    header = xml.encode("utf-16-le") + b"\x00\x00"
    out = struct.pack(">I", len(header)) + header + struct.pack("<I", zlib.adler32(header))
    path.write_bytes(out + kw + info_block + key_blocks + rec)


def header_only(path, attrs):
    xml = "<Dictionary " + " ".join(f'{k}="{v}"' for k, v in attrs.items()) + "/>\r\n"
    header = xml.encode("utf-16-le") + b"\x00\x00"
    path.write_bytes(struct.pack(">I", len(header)) + header + struct.pack("<I", zlib.adler32(header)) + b"\x00" * 64)


# ---------------------------------------------------------------- 词条

long_html = "<p>" + "；".join(f"义项 {i}：在很多场合都会出现的" for i in range(1, 9)) + "</p>"

(HERE / "basic").mkdir(exist_ok=True)
write_mdx(
    HERE / "basic" / "basic.mdx",
    [
        ("ubiquitous", "`1`ubiquitous`2`adj. 无处不在的"),
        ("ubiquity", "@@@LINK=ubiquitous"),
        ("omnipresent", long_html),
        ("bank", "<p>河岸</p>"),
        ("bank", "<p>银行</p>"),
        ("Polish", "<p>波兰的</p>"),
        ("polish", "<p>擦亮</p>"),
        ("color", '<p>颜色 <img src="img/dot.png"> <a href="sound://ubiquitous.mp3">🔊</a></p>'),
        ("colour", "@@@LINK=color"),
        ("Colour", "@@@LINK=colour"),
        ("loop1", "@@@LINK=loop2"),
        ("loop2", "@@@LINK=loop1"),
        ("café", "<p>咖啡（馆）</p>"),
    ],
    encrypted=2,
    stylesheet="1\r\n<b>\r\n</b>\r\n2\r\n<i>\r\n</i>",
    title="Basic（测试）",
)
write_mdx(
    HERE / "basic" / "basic.mdd",
    [("\\style.css", b"p { color: #333 }"), ("\\img\\Dot.png", b"\x89PNG\r\n\x1a\nfake")],
    is_mdd=True,
)
write_mdx(HERE / "basic" / "basic.1.mdd", [("\\ubiquitous.mp3", b"ID3fake-audio")], is_mdd=True)
# 散装资源：mdx 同目录的 CSS（真实词典常见做法），同名文件优先于 mdd
(HERE / "basic" / "basic.css").write_bytes(b".hw { font-weight: bold }")
(HERE / "basic" / "style.css").write_bytes(b"/* loose overrides mdd */")

(HERE / "legacy").mkdir(exist_ok=True)
write_mdx(
    HERE / "legacy" / "legacy.mdx",
    [
        ("中国", "<p>China</p>"),
        ("汉语", "<p>Chinese language</p>"),
        ("abandon", "<p>放弃；抛弃</p>"),
        ("zeal", "<p>热情</p>"),
        ("中文", "@@@LINK=汉语"),
    ],
    version="1.2",
    encoding="GBK",
    ctype=1,
    keys_per_block=2,
    record_block=32,
    title="Legacy（测试）",
)

(HERE / "utf16").mkdir(exist_ok=True)
write_mdx(
    HERE / "utf16" / "utf16.mdx",
    [("hello", "<p>你好</p>"), ("naïve", "<p>天真的</p>"), ("world", "<p>世界</p>")],
    encoding="UTF-16",
    ctype=0,
    title="",
)

header_only(HERE / "regcode.mdx", {"GeneratedByEngineVersion": "2.0", "Encrypted": "1", "Encoding": "UTF-8"})
header_only(HERE / "v3.mdx", {"GeneratedByEngineVersion": "3.0", "Encrypted": "0", "Encoding": "UTF-8"})

print("fixtures written to", HERE)
