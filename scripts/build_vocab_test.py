#!/usr/bin/env python3
"""生成词汇量测试题库 src-tauri/data/vocab_test.json.gz。

    py -3 -I scripts/build_vocab_test.py <ecdict.csv> src-tauri/data/vocab_test.json.gz

真词按内置词频表（wordfreq 3.1）的排名分段抽取，释义取自 ECDICT；假词由音节模板组合，
并逐个确认不在 WordNet、词频表、完整 ECDICT（含变形）里。
只读 ECDICT 的 word、translation、exchange 三列，不用它的词频列。
"""

import csv
import gzip
import io
import json
import random
import re
import sys
from pathlib import Path

VERSION = 1
SEED = 20261119
PER_BAND = 30
FAKE_COUNT = 150
DATA = Path(__file__).resolve().parent.parent / "src-tauri" / "data"
# 每段的结束排名：阅读常用的 2000–12000 分得更细
BAND_ENDS = [500, 1000, 1500, 2000, 2500, 3000, 4000, 5000, 6000, 7000, 8000, 10000, 12000, 14000, 16000, 20000, 24000, 28000, 34000, 40000]
MAX_GLOSS = 12
# 不适合出现在测试里的词
EXCLUDE = {
    "fuck", "fucking", "shit", "bitch", "cunt", "dick", "cock", "pussy", "whore", "slut", "nigger", "negro", "faggot", "fag", "retard",
    "rape", "rapist", "porn", "porno", "penis", "vagina", "anus", "semen", "orgasm", "masturbate", "nazi", "bastard", "asshole", "tits",
    "boob", "boobs", "piss", "crap", "damn", "hell", "sex", "sexy", "horny", "blowjob", "dildo", "incest", "pedophile", "genocide",
}
NAME_LIKE = re.compile(r"人名|地名|姓氏|缩写|商标|男子名|女子名")
HAS_LATIN = re.compile(r"[A-Za-z]")
# 人工审阅后去掉的词（人名、地名、品牌、缩写、敏感词）
REVIEW_DROP = set("""liv chang johnny russell zealand serbia sofia ahmad bosnia burgess avon mercer amish atkins
eminem msnbc nikon emanuel fisk boswell chiu folsom herbie lauder mysore snell adama brea
greeley ino loup oviedo sippy tirana zippo begum vesta han que cum facebook rep
prix piper wheeler ping ugh gosh hey condom bisexual homosexual genitalia prat tech apps
info neuro addon headquarter unarm soma nova vietnamese coronavirus mullah shinto bonito agar grange""".split())
# 人工审阅后改写的释义：ECDICT 的第一个义项不是最常用的意思
OVERRIDES = {
    "fine": "好的", "while": "当…的时候", "feature": "特征", "formula": "公式", "modern": "现代的", "review": "评论",
    "process": "过程", "live": "生活", "general": "一般的", "close": "关闭", "gut": "肠子", "medic": "医生",
    "hopeful": "有希望的", "interpreter": "口译员", "makeup": "化妆品", "scary": "可怕的", "meditate": "冥想", "mechanically": "机械地",
    "progressively": "逐步地", "bouncer": "门口保镖", "igneous": "火成的", "concussion": "脑震荡", "coaster": "杯垫", "relay": "接力",
    "tear": "眼泪", "zip": "拉链", "hood": "兜帽", "icon": "图标", "implementation": "实施", "implement": "实施",
    "express": "表达", "exhibit": "展览", "extract": "提取", "dependent": "依赖的", "festival": "节日", "binary": "二进制的",
    "thriller": "惊悚片", "addict": "成瘾者", "shatter": "粉碎", "recorder": "录音机", "missile": "导弹", "martial": "军事的",
    "bolt": "螺栓", "resolution": "决心", "supreme": "最高的", "agent": "代理人", "critical": "关键的", "grand": "宏伟的",
    "truly": "真正地", "chicken": "鸡", "yard": "院子", "rally": "集会", "legacy": "遗产", "objective": "目标",
    "ultimate": "最终的", "feed": "喂养", "trail": "小径", "unknown": "未知的", "gap": "差距", "shock": "震惊",
    "aggressive": "好斗的", "apparent": "明显的", "frequent": "频繁的", "graphic": "图形的", "phrase": "短语", "stability": "稳定",
    "nightmare": "噩梦", "essence": "本质", "compulsory": "强制的", "valuation": "估值", "sincere": "真诚的", "eternity": "永恒",
    "bliss": "极乐", "synthesis": "合成", "extraction": "提取", "crusade": "十字军东征", "eminent": "杰出的", "drastic": "剧烈的",
    "plausible": "貌似合理的", "sewage": "污水", "kettle": "水壶", "renovate": "翻新", "vastly": "极大地", "cyclone": "气旋",
    "conglomerate": "企业集团", "duly": "按时地", "provocative": "挑衅的", "stout": "粗壮的", "turnaround": "好转", "recourse": "求助",
    "forfeit": "丧失", "foundry": "铸造厂", "chopper": "直升机", "couture": "高级时装", "brisk": "轻快的", "manipulative": "善于操纵的",
    "maniac": "疯子", "northward": "向北", "rejoin": "重新加入", "flashy": "浮华的", "distillery": "酿酒厂", "excavate": "挖掘",
    "dispatcher": "调度员", "catchment": "集水区", "juggernaut": "不可阻挡的力量", "trope": "修辞手法", "understate": "轻描淡写", "unpack": "打开行李",
    "epithet": "绰号", "eraser": "橡皮擦", "remover": "去除剂", "vindicate": "证明无辜", "soulless": "没有灵魂的", "tuner": "调谐器",
    "lurid": "耸人听闻的", "bootstrap": "自力更生", "teamster": "卡车司机", "throwaway": "一次性的", "overhang": "悬垂", "swank": "炫耀",
    "indigent": "贫困的", "rickety": "摇摇晃晃的", "sorbet": "冰沙", "effluent": "废水", "confidante": "女密友", "bogie": "转向架",
    "soundness": "健全", "concuss": "使脑震荡", "tarry": "逗留", "hibernate": "冬眠", "headcount": "人数", "bushland": "灌木林地",
    "sweatshirt": "运动衫", "chalet": "木屋", "cheddar": "切达干酪", "mezzanine": "夹层楼面", "consumer": "消费者",
}
POS = re.compile(r"^(?:n|v|vt|vi|a|adj|ad|adv|prep|conj|pron|int|interj|num|art|aux)\.\s*")
ONSETS = "b bl br c cl cr d dr f fl fr g gl gr h j k l m n p pl pr qu r s sc sk sl sm sn sp st str t th tr v w wh z".split()
VOWELS = "a e i o u ea oa".split()
MIDDLES = "b d f g l m n p r s t v z".split()
ENDINGS = "al ance ate en er et ic ine ish ist ity ive let ment ness on or ous ure y".split()


def load_freq() -> list[str]:
    with gzip.open(DATA / "wordfreq_en.txt.gz", "rt", encoding="utf-8") as f:
        return [w for w in (line.strip() for line in f) if w and not w.startswith("#")]


def load_wordnet() -> set[str]:
    with gzip.open(DATA / "wordnet.tsv.gz", "rt", encoding="utf-8") as f:
        return {t for line in f for t in re.findall(r"[a-z]+", line.lower())}


def gloss_of(translation: str) -> str | None:
    """第一条带词性的释义里的第一个义项"""
    for line in translation.split("\\n"):
        line = line.strip()
        if not POS.match(line):
            continue
        first = re.split(r"[,，;；、]", POS.sub("", line))[0]
        first = re.sub(r"[\[（(<【].*?[\]）)>】]", "", first).strip(" .…")
        if first and len(first) <= MAX_GLOSS and re.search(r"[一-鿿]", first) and not HAS_LATIN.search(first):
            return first
        return None
    return None


def load_ecdict(path: str) -> tuple[dict[str, str], set[str]]:
    csv.field_size_limit(10_000_000)
    glosses: dict[str, str] = {}
    every: set[str] = set()
    with open(path, encoding="utf-8", newline="") as f:
        for row in csv.DictReader(f):
            word, translation, exchange = row["word"], row["translation"], row["exchange"]
            every.add(word.lower())
            every.update(item.partition(":")[2].lower() for item in exchange.split("/") if ":" in item)
            if NAME_LIKE.search(translation):
                continue
            gloss = gloss_of(translation)
            if gloss:
                glosses[word] = gloss
    return glosses, every


def make_bands(freq: list[str], glosses: dict[str, str], rng: random.Random) -> list[dict]:
    bands, start = [], 1
    for end in BAND_ENDS:
        pool = [w for w in freq[start - 1 : end] if len(w) >= 3 and w.isascii() and w.isalpha() and w.islower() and w in glosses and w not in EXCLUDE]
        picked = sorted(rng.sample(pool, min(PER_BAND, len(pool))))
        words = [{"w": w, "zh": OVERRIDES.get(w, glosses[w])} for w in picked if w not in REVIEW_DROP]
        bands.append({"from": start, "to": end, "words": words})
        start = end + 1
    return bands


def make_fakes(real: set[str], rng: random.Random) -> list[str]:
    fakes: set[str] = set()
    while len(fakes) < FAKE_COUNT:
        w = rng.choice(ONSETS) + rng.choice(VOWELS) + rng.choice(MIDDLES) + rng.choice(ENDINGS)
        if not 5 <= len(w) <= 10 or any(b in w for b in EXCLUDE if len(b) > 3):
            continue
        # 去掉常见词尾后也不能是真词（plomets、plometer 这类）
        stems = {w, w[:-1], w[:-2], w[:-3], w + "s", w + "e", w + "ed", w + "ing"}
        if not stems & real:
            fakes.add(w)
    return sorted(fakes)


def main() -> None:
    src, dest = sys.argv[1], sys.argv[2]
    rng = random.Random(SEED)
    freq = load_freq()
    glosses, every = load_ecdict(src)
    real = every | set(freq) | load_wordnet()
    bands = make_bands(freq, glosses, rng)
    fakes = make_fakes(real, rng)
    assert not set(fakes) & real
    out = io.BytesIO()
    with gzip.GzipFile(fileobj=out, mode="wb", mtime=0, compresslevel=9) as z:
        z.write(json.dumps({"version": VERSION, "bands": bands, "fakes": fakes}, ensure_ascii=False, separators=(",", ":")).encode("utf-8"))
    Path(dest).write_bytes(out.getvalue())
    sys.stdout.reconfigure(encoding="utf-8")
    for b in bands:
        print(b["from"], b["to"], len(b["words"]), " ".join(f'{x["w"]}={x["zh"]}' for x in b["words"][:5]))
    print("fakes", len(fakes), " ".join(fakes[:40]))
    print("bytes", len(out.getvalue()))


if __name__ == "__main__":
    main()
