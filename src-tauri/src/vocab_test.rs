//! 词汇量测试：出题和估算（纯函数，不碰数据库和界面）。
//!
//! 题库按内置词频表的排名分段（`scripts/build_vocab_test.py` 生成）。出题是无状态的：同样的种子和答题记录
//! 总是得到同样的下一题，界面每答一题把全部记录传回来。
//!
//! 估算：假设认识率随排名按 `1 / (1 + (排名 / c)^s)` 下降，再加一个「不认识也说认识」的比例 g。在 (c, s, g) 的
//! 网格上按全部作答（真词、假词、释义抽查）算后验分布，词汇量取后验中位数，误差范围取后验的 5%–95%。

use std::collections::HashSet;
use std::io::Read;
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

pub const MAX_VOCAB: u32 = 40_000;
/// 词题总数（含假词）
pub const WORD_QUESTIONS: u32 = 60;
/// 开头这么多题覆盖各频段，粗略定位水平
const LOCATE_QUESTIONS: u32 = 12;
pub const FAKE_QUESTIONS: u32 = 8;
pub const MAX_CHECKS: u32 = 10;
/// 假词答「认识」的比例达到这个数，提示结果可能偏乐观
const OPTIMISTIC_FAKE_RATE: f64 = 0.25;

const DATA: &[u8] = include_bytes!("../data/vocab_test.json.gz");

#[derive(Debug, Deserialize)]
pub struct Item {
    pub w: String,
    pub zh: String,
}

#[derive(Debug, Deserialize)]
pub struct Band {
    pub from: u32,
    pub to: u32,
    pub words: Vec<Item>,
}

#[derive(Debug, Deserialize)]
pub struct Bank {
    pub version: u32,
    pub bands: Vec<Band>,
    pub fakes: Vec<String>,
}

pub fn bank() -> &'static Bank {
    static BANK: OnceLock<Bank> = OnceLock::new();
    BANK.get_or_init(|| {
        let mut text = String::new();
        flate2::read::GzDecoder::new(DATA).read_to_string(&mut text).expect("内置题库损坏");
        serde_json::from_str(&text).expect("内置题库损坏")
    })
}

pub fn bank_version() -> u32 {
    bank().version
}

/// 一道词题的作答。`band` = None 表示假词；`check` = 释义抽查的结果（没抽查是 None，选「不确定」算答错）
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Answer {
    pub word: String,
    pub band: Option<usize>,
    pub said_known: bool,
    pub check: Option<bool>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Next {
    Word { word: String, band: Option<usize>, index: u32, total: u32 },
    Check { word: String, options: [String; 3], correct: u8, index: u32, total: u32 },
    Done,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Estimate {
    pub value: u32,
    pub low: u32,
    pub high: u32,
    /// 达到了测量上限
    pub capped: bool,
    /// 假词认得太多，这次结果可能偏乐观
    pub optimistic: bool,
    /// 各词频段大约认识多少（按拟合出的曲线算，比逐题统计稳）
    pub profile: Vec<Segment>,
    /// 答题情况
    pub stats: Stats,
}

/// 结果页「词汇分布」的分段（排名区间）
pub const SEGMENTS: [(u32, u32); 5] = [(1, 2000), (2001, 5000), (5001, 10_000), (10_001, 20_000), (20_001, MAX_VOCAB)];

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Segment {
    pub from: u32,
    pub to: u32,
    /// 这一段认识的比例 0–1
    pub known: f64,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Stats {
    /// 真词题数 / 答「认识」的
    pub real: u32,
    pub real_known: u32,
    /// 假词题数 / 识破的（答「不认识」）
    pub fakes: u32,
    pub fakes_caught: u32,
    /// 释义抽查题数 / 答对的
    pub checks: u32,
    pub checks_right: u32,
}

fn stats_of(answers: &[Answer]) -> Stats {
    let mut s = Stats::default();
    for a in answers {
        if a.band.is_some() {
            s.real += 1;
            s.real_known += a.said_known as u32;
        } else {
            s.fakes += 1;
            s.fakes_caught += !a.said_known as u32;
        }
        if let Some(c) = a.check {
            s.checks += 1;
            s.checks_right += c as u32;
        }
    }
    s
}

/// 按后验把各题库段的认识率平均，再按排名区间重叠的长度摊到结果页的分段上
fn profile_of(post: &[f64]) -> Vec<Segment> {
    let bands = &bank().bands;
    let mut know = vec![0.0; bands.len()];
    for (c, p) in grid().iter().zip(post) {
        for (k, ck) in know.iter_mut().zip(&c.know) {
            *k += p * ck;
        }
    }
    SEGMENTS
        .iter()
        .map(|&(from, to)| {
            let (mut sum, mut len) = (0.0, 0.0);
            for (b, k) in bands.iter().zip(&know) {
                let lo = b.from.max(from);
                let hi = b.to.min(to);
                if lo <= hi {
                    let n = (hi - lo + 1) as f64;
                    sum += k * n;
                    len += n;
                }
            }
            Segment { from, to, known: if len > 0.0 { sum / len } else { 0.0 } }
        })
        .collect()
}

/// SplitMix64：由种子和序号算出确定的随机数
fn mix(a: u64, b: u64) -> u64 {
    let mut z = a.wrapping_add(b.wrapping_mul(0x9E37_79B9_7F4A_7C15)).wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// 第几题是假词：都放在定位题之后
fn fake_slots(seed: u64) -> Vec<u32> {
    let mut slots: Vec<u32> = (LOCATE_QUESTIONS..WORD_QUESTIONS).collect();
    for i in 0..FAKE_QUESTIONS as usize {
        let j = i + (mix(seed, 1000 + i as u64) % (slots.len() - i) as u64) as usize;
        slots.swap(i, j);
    }
    slots.truncate(FAKE_QUESTIONS as usize);
    slots
}

/// 从候选里挑一个没考过的；`avoid` 里的词只有在别无选择时才用
fn pick<'a>(pool: &[&'a str], asked: &HashSet<&str>, avoid: &HashSet<String>, h: u64) -> Option<&'a str> {
    let fresh: Vec<&str> = pool.iter().copied().filter(|w| !asked.contains(w) && !avoid.contains(*w)).collect();
    let open: Vec<&str> = pool.iter().copied().filter(|w| !asked.contains(w)).collect();
    let from = if fresh.is_empty() { &open } else { &fresh };
    (!from.is_empty()).then(|| from[(h % from.len() as u64) as usize])
}

/// 每段的作答统计
#[derive(Clone, Copy, Default)]
struct Tally {
    asked: u32,
    known: u32,
    checks_right: u32,
    checks_wrong: u32,
}

/// 网格上的一个点：一种可能的答题者
struct Candidate {
    /// 每段词的平均认识率
    know: Vec<f64>,
    /// 这个答题者的词汇量
    vocab: f64,
    /// 不认识也说认识的比例
    guess: f64,
    log_prior: f64,
}

fn grid() -> &'static [Candidate] {
    static GRID: OnceLock<Vec<Candidate>> = OnceLock::new();
    GRID.get_or_init(|| {
        const CENTERS: usize = 80;
        let (lo, hi) = (30f64.ln(), 200_000f64.ln());
        let mut out = Vec::new();
        for i in 0..CENTERS {
            let c = (lo + (hi - lo) * i as f64 / (CENTERS - 1) as f64).exp();
            for s in [2.0, 2.5, 3.0, 3.5, 4.0] {
                let know: Vec<f64> = bank()
                    .bands
                    .iter()
                    .map(|b| {
                        // 段内按对数取几个点求平均：第一段跨度很大，只取中点不准
                        let (a, z) = ((b.from as f64).ln(), (b.to as f64).ln());
                        (0..4).map(|k| 1.0 / (1.0 + (((a + (z - a) * (k as f64 + 0.5) / 4.0).exp()) / c).powf(s))).sum::<f64>() / 4.0
                    })
                    .collect();
                let vocab = know.iter().zip(&bank().bands).map(|(k, b)| k * (b.to - b.from + 1) as f64).sum();
                for guess in [0.0, 0.05, 0.1, 0.15, 0.2, 0.3, 0.4, 0.5, 0.7] {
                    // 大多数人不会乱答，g 越大先验越小
                    out.push(Candidate { know: know.clone(), vocab, guess, log_prior: -guess / 0.4 });
                }
            }
        }
        out
    })
}

/// 按作答算网格上每个点的后验概率
fn posterior(answers: &[Answer]) -> Vec<f64> {
    let mut tally = vec![Tally::default(); bank().bands.len()];
    let (mut fake_yes, mut fake_no) = (0.0, 0.0);
    for a in answers {
        match a.band.and_then(|b| tally.get_mut(b)) {
            Some(t) => {
                t.asked += 1;
                t.known += a.said_known as u32;
                match a.check {
                    Some(true) => t.checks_right += 1,
                    Some(false) => t.checks_wrong += 1,
                    None => {}
                }
            }
            None if a.said_known => fake_yes += 1.0,
            None => fake_no += 1.0,
        }
    }
    let logs: Vec<f64> = grid()
        .iter()
        .map(|c| {
            let g = c.guess.max(0.01);
            let mut l = c.log_prior + fake_yes * g.ln() + fake_no * (1.0 - g).ln();
            for (t, k) in tally.iter().zip(&c.know) {
                let yes = (c.guess + (1.0 - c.guess) * k).clamp(0.01, 0.99);
                l += t.known as f64 * yes.ln() + (t.asked - t.known) as f64 * (1.0 - yes).ln();
                if t.checks_right + t.checks_wrong > 0 {
                    // 说认识的词里真认识的比例；不认识时三选一猜中三分之一
                    let real = (k / yes).min(1.0);
                    let right = (real + (1.0 - real) / 3.0).clamp(0.01, 0.99);
                    l += t.checks_right as f64 * right.ln() + t.checks_wrong as f64 * (1.0 - right).ln();
                }
            }
            l
        })
        .collect();
    let top = logs.iter().copied().fold(f64::MIN, f64::max);
    let weights: Vec<f64> = logs.iter().map(|l| (l - top).exp()).collect();
    let sum: f64 = weights.iter().sum();
    weights.iter().map(|w| w / sum).collect()
}

/// 下一道真词出在哪一段：选答案最能缩小词汇量误差的那段
fn next_band(answers: &[Answer], asked: &HashSet<&str>, avoid: &HashSet<String>) -> usize {
    let post = posterior(answers);
    let mean_v: f64 = post.iter().zip(grid()).map(|(p, c)| p * c.vocab).sum();
    // 优先选还有新词的段；都考过了才回头用最近考过的词
    let open = |b: &Band, skip_recent: bool| b.words.iter().any(|w| !asked.contains(w.w.as_str()) && !(skip_recent && avoid.contains(&w.w)));
    let skip_recent = bank().bands.iter().any(|b| open(b, true));
    let mut best = (f64::MIN, 0);
    for (i, b) in bank().bands.iter().enumerate() {
        if !open(b, skip_recent) {
            continue;
        }
        let (mut mean_y, mut cov) = (0.0, 0.0);
        for (p, c) in post.iter().zip(grid()) {
            let y = c.guess + (1.0 - c.guess) * c.know[i];
            mean_y += p * y;
            cov += p * y * (c.vocab - mean_v);
        }
        let score = cov * cov / (mean_y * (1.0 - mean_y) + 1e-6);
        if score > best.0 {
            best = (score, i);
        }
    }
    best.1
}

/// 这道答「认识」的词要不要抽查释义。只看这题之前的记录，所以重算结果不变
fn wants_check(seed: u64, answers: &[Answer]) -> bool {
    let Some((last, before)) = answers.split_last() else { return false };
    if last.band.is_none() || !last.said_known {
        return false;
    }
    let done = before.iter().filter(|a| a.check.is_some()).count() as f64;
    let left = MAX_CHECKS as f64 - done;
    if left <= 0.0 {
        return false;
    }
    let real: Vec<&Answer> = before.iter().filter(|a| a.band.is_some()).collect();
    let known = real.iter().filter(|a| a.said_known).count() as f64;
    let rate = (known + 1.0) / (real.len() as f64 + 2.0);
    // 后面大约还会有多少道答「认识」的题
    let remaining = (WORD_QUESTIONS as f64 - answers.len() as f64 + 1.0) * rate;
    (mix(seed, 2000 + answers.len() as u64) % 10_000) as f64 / 10_000.0 < left / remaining.max(1.0)
}

fn check_question(seed: u64, last: &Answer, index: u32) -> Option<Next> {
    let band = &bank().bands[last.band?];
    let gloss = &band.words.iter().find(|w| w.w == last.word)?.zh;
    let mut others: Vec<&String> = Vec::new();
    let mut k = 0;
    // 干扰项优先取同段的释义，同段不够再往邻段找
    let all: Vec<&Item> = band.words.iter().chain(bank().bands.iter().flat_map(|b| b.words.iter())).collect();
    while others.len() < 2 && k < all.len() * 4 {
        let h = mix(seed, 3000 + index as u64 * 64 + k as u64);
        let range = if k < band.words.len() * 2 { band.words.len() } else { all.len() };
        let c = &all[(h % range as u64) as usize].zh;
        if c != gloss && !others.contains(&c) {
            others.push(c);
        }
        k += 1;
    }
    let (a, b) = (others.first()?.to_string(), others.get(1)?.to_string());
    let correct = (mix(seed, 4000 + index as u64) % 3) as u8;
    let mut options = [a, b, gloss.clone()];
    options.swap(2, correct as usize);
    Some(Next::Check { word: last.word.clone(), options, correct, index, total: WORD_QUESTIONS })
}

/// 下一题。相同的输入总是返回相同的结果
pub fn next(seed: u64, answers: &[Answer], avoid: &HashSet<String>) -> Next {
    let index = answers.len() as u32;
    if let Some(last) = answers.last() {
        if last.check.is_none() && wants_check(seed, answers) {
            if let Some(q) = check_question(seed, last, index) {
                return q;
            }
        }
    }
    if index >= WORD_QUESTIONS {
        return Next::Done;
    }
    let asked: HashSet<&str> = answers.iter().map(|a| a.word.as_str()).collect();
    let h = mix(seed, index as u64);
    if fake_slots(seed).contains(&index) {
        let pool: Vec<&str> = bank().fakes.iter().map(String::as_str).collect();
        if let Some(w) = pick(&pool, &asked, avoid, h) {
            return Next::Word { word: w.to_string(), band: None, index, total: WORD_QUESTIONS };
        }
    }
    let bands = &bank().bands;
    let spread = (index as usize * (bands.len() - 1)) / (LOCATE_QUESTIONS.max(2) as usize - 1);
    // 定位题均匀覆盖各段；这一段的词最近都考过了，就换一段
    let fresh = |b: usize| bands[b].words.iter().any(|w| !asked.contains(w.w.as_str()) && !avoid.contains(&w.w));
    let band = if index < LOCATE_QUESTIONS && fresh(spread) { spread } else { next_band(answers, &asked, avoid) };
    let pool: Vec<&str> = bands[band].words.iter().map(|w| w.w.as_str()).collect();
    match pick(&pool, &asked, avoid, h) {
        Some(w) => Next::Word { word: w.to_string(), band: Some(band), index, total: WORD_QUESTIONS },
        None => Next::Done,
    }
}

/// 估算词汇量和 90% 区间
pub fn estimate(answers: &[Answer], _seed: u64) -> Estimate {
    let real: Vec<&Answer> = answers.iter().filter(|a| a.band.is_some()).collect();
    let fakes: Vec<&Answer> = answers.iter().filter(|a| a.band.is_none()).collect();
    let fake_known = fakes.iter().filter(|a| a.said_known).count();
    let fa = if fakes.is_empty() { 0.0 } else { fake_known as f64 / fakes.len() as f64 };
    let optimistic = fa >= OPTIMISTIC_FAKE_RATE;
    let none_known = !real.iter().any(|a| a.said_known);
    let stats = stats_of(answers);
    if none_known {
        let profile = SEGMENTS.iter().map(|&(from, to)| Segment { from, to, known: 0.0 }).collect();
        return Estimate { value: 0, low: 0, high: 0, capped: false, optimistic, profile, stats };
    }
    let post = posterior(answers);
    let mut by_vocab: Vec<(f64, f64)> = grid().iter().zip(&post).map(|(c, p)| (c.vocab, *p)).collect();
    by_vocab.sort_by(|a, b| a.0.total_cmp(&b.0));
    let quantile = |q: f64| {
        let mut acc = 0.0;
        for (v, p) in &by_vocab {
            acc += p;
            if acc >= q {
                return *v;
            }
        }
        MAX_VOCAB as f64
    };
    // 全都认识、假词也没上当：已经到了这套题能测的上限
    let top = real.iter().all(|a| a.said_known) && fake_known == 0;
    let value = if top { MAX_VOCAB } else { quantile(0.5).round().clamp(0.0, MAX_VOCAB as f64) as u32 };
    let low = (quantile(0.05).round() as u32).min(value);
    let high = (quantile(0.95).round() as u32).clamp(value, MAX_VOCAB);
    let profile = profile_of(&post);
    Estimate { value, low, high, capped: value >= MAX_VOCAB, optimistic, profile, stats }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Copy, PartialEq)]
    enum Kind {
        Honest,
        Over,
        Under,
    }

    fn roll(seed: u64, word: &str, salt: u64) -> f64 {
        let h = word.bytes().fold(seed ^ salt, |a, b| mix(a, b as u64));
        (h % 10_000) as f64 / 10_000.0
    }

    /// 排名 r 的词认识的概率
    fn p_know(r: f64, center: f64) -> f64 {
        1.0 / (1.0 + (r / center).powi(3))
    }

    fn truth(center: f64) -> f64 {
        (1..=MAX_VOCAB).step_by(10).map(|r| p_know(r as f64, center) * 10.0).sum()
    }

    fn center_for(v: f64) -> f64 {
        let (mut lo, mut hi) = (10.0, 400_000.0);
        for _ in 0..40 {
            let m = (lo * hi as f64).sqrt();
            if truth(m) < v {
                lo = m;
            } else {
                hi = m;
            }
        }
        lo
    }

    /// 模拟一个答题者答完整套题
    fn simulate(seed: u64, center: f64, kind: Kind) -> Vec<Answer> {
        let mut answers: Vec<Answer> = Vec::new();
        let none = HashSet::new();
        loop {
            match next(seed, &answers, &none) {
                Next::Word { word, band, .. } => {
                    let said_known = match band {
                        None => kind == Kind::Over && roll(seed, &word, 1) < 0.3,
                        Some(b) => {
                            let bd = &bank().bands[b];
                            let knows = roll(seed, &word, 2) < p_know(((bd.from * bd.to) as f64).sqrt(), center);
                            match kind {
                                Kind::Honest => knows,
                                Kind::Over => knows || roll(seed, &word, 3) < 0.3,
                                Kind::Under => knows && roll(seed, &word, 4) > 0.08,
                            }
                        }
                    };
                    answers.push(Answer { word, band, said_known, check: None });
                }
                Next::Check { word, .. } => {
                    let last = answers.last_mut().unwrap();
                    assert_eq!(last.word, word);
                    let bd = &bank().bands[last.band.unwrap()];
                    let knows = roll(seed, &word, 2) < p_know(((bd.from * bd.to) as f64).sqrt(), center);
                    last.check = Some(knows || roll(seed, &word, 5) < 1.0 / 3.0);
                }
                Next::Done => return answers,
            }
        }
    }

    fn fixed(seed: u64, known: bool, fakes_known: bool, check: Option<bool>) -> Vec<Answer> {
        let mut answers: Vec<Answer> = Vec::new();
        let none = HashSet::new();
        loop {
            match next(seed, &answers, &none) {
                Next::Word { word, band, .. } => {
                    let said_known = if band.is_none() { fakes_known } else { known };
                    answers.push(Answer { word, band, said_known, check: None });
                }
                Next::Check { .. } => answers.last_mut().unwrap().check = Some(check.unwrap_or(true)),
                Next::Done => return answers,
            }
        }
    }

    #[test]
    fn next_is_deterministic_and_finishes() {
        let a = simulate(7, 6000.0, Kind::Honest);
        let b = simulate(7, 6000.0, Kind::Honest);
        assert_eq!(a.iter().map(|x| &x.word).collect::<Vec<_>>(), b.iter().map(|x| &x.word).collect::<Vec<_>>());
        assert_eq!(a.len(), WORD_QUESTIONS as usize);
        assert_eq!(a.iter().filter(|x| x.band.is_none()).count(), FAKE_QUESTIONS as usize);
        let checks = a.iter().filter(|x| x.check.is_some()).count();
        assert!((1..=MAX_CHECKS as usize).contains(&checks), "抽查 {checks} 次");
        let words: HashSet<_> = a.iter().map(|x| &x.word).collect();
        assert_eq!(words.len(), a.len(), "同一次测试不重复出题");
    }

    #[test]
    fn check_offers_the_right_gloss() {
        let mut answers = Vec::new();
        let none = HashSet::new();
        for _ in 0..80 {
            match next(11, &answers, &none) {
                Next::Word { word, band, .. } => answers.push(Answer { word, band, said_known: band.is_some(), check: None }),
                Next::Check { word, options, correct, .. } => {
                    let gloss = bank().bands.iter().flat_map(|b| &b.words).find(|i| i.w == word).unwrap().zh.clone();
                    assert_eq!(options[correct as usize], gloss);
                    assert_eq!(options.iter().collect::<HashSet<_>>().len(), 3, "三个选项互不相同");
                    return;
                }
                Next::Done => break,
            }
        }
        panic!("没有出现抽查题");
    }

    #[test]
    fn avoids_recent_words() {
        let first: HashSet<String> = simulate(3, 6000.0, Kind::Honest).into_iter().map(|a| a.word).collect();
        let mut answers: Vec<Answer> = Vec::new();
        loop {
            match next(4, &answers, &first) {
                Next::Word { word, band, .. } => {
                    assert!(!first.contains(&word), "{word} 上次考过");
                    answers.push(Answer { word, band, said_known: false, check: None });
                }
                Next::Check { .. } => answers.last_mut().unwrap().check = Some(true),
                Next::Done => break,
            }
        }
    }

    #[test]
    fn extreme_answers_stay_in_range() {
        for (known, fakes, check) in [(true, false, None), (false, false, None), (true, true, None), (true, false, Some(false)), (false, true, None)] {
            let e = estimate(&fixed(5, known, fakes, check), 5);
            assert!(e.value <= MAX_VOCAB && e.low <= e.value && e.value <= e.high && e.high <= MAX_VOCAB, "{e:?}");
        }
        let all = estimate(&fixed(5, true, false, None), 5);
        assert!(all.capped && all.value == MAX_VOCAB, "{all:?}");
        assert_eq!(estimate(&fixed(5, false, false, None), 5).value, 0);
        assert_eq!(estimate(&[], 5).value, 0, "没有答题记录时不崩溃");
    }

    #[test]
    fn fake_words_lower_the_estimate() {
        let mut lied = simulate(9, 6000.0, Kind::Honest);
        let clean = estimate(&lied, 9);
        for a in lied.iter_mut().filter(|a| a.band.is_none()) {
            a.said_known = true;
        }
        let e = estimate(&lied, 9);
        assert!(e.value < clean.value && e.optimistic, "{e:?} vs {clean:?}");
        assert!(!clean.optimistic);
    }

    #[test]
    fn failed_checks_lower_the_estimate() {
        let mut a = simulate(13, 8000.0, Kind::Honest);
        let clean = estimate(&a, 13);
        for x in a.iter_mut().filter(|x| x.check.is_some()) {
            x.check = Some(false);
        }
        assert!(estimate(&a, 13).value < clean.value);
    }

    /// 约 52 道真词的信息量有上限：认真作答时词汇量的标准差约 11%，所以按 ±15% 和 ±25% 分别验收
    #[test]
    fn simulated_learners_are_accurate() {
        let (mut total, mut within15, mut within25, mut covered) = (0, 0, 0, 0);
        let (mut honest, mut honest15) = (0, 0);
        for v in [2000.0, 5000.0, 8000.0, 12000.0, 20000.0] {
            let center = center_for(v);
            for kind in [Kind::Honest, Kind::Over, Kind::Under] {
                for seed in 0..10 {
                    let e = estimate(&simulate(seed * 31 + 1, center, kind), seed);
                    let err = (e.value as f64 - v).abs() / v;
                    total += 1;
                    within15 += (err <= 0.15) as u32;
                    within25 += (err <= 0.25) as u32;
                    covered += (e.low as f64 <= v && v <= e.high as f64) as u32;
                    if kind == Kind::Honest {
                        honest += 1;
                        honest15 += (err <= 0.15) as u32;
                    }
                }
            }
        }
        let rate = |n: u32, of: u32| n as f64 / of as f64;
        let report = format!("15%: {within15}/{total}, 25%: {within25}/{total}, honest 15%: {honest15}/{honest}, covered: {covered}/{total}");
        assert!(rate(honest15, honest) >= 0.8, "{report}");
        assert!(rate(within15, total) >= 0.7, "{report}");
        assert!(rate(within25, total) >= 0.9, "{report}");
        assert!(rate(covered, total) >= 0.75, "误差范围要盖住真实值：{report}");
    }

    #[test]
    fn profile_falls_with_rank_and_stats_count_answers() {
        let e = estimate(&simulate(7, center_for(6000.0), Kind::Honest), 7);
        assert_eq!(e.profile.len(), SEGMENTS.len());
        assert_eq!(e.profile[0].from, 1);
        assert_eq!(e.profile.last().unwrap().to, MAX_VOCAB);
        assert!(e.profile.windows(2).all(|w| w[0].known >= w[1].known), "{:?}", e.profile);
        assert!(e.profile[0].known > 0.9 && e.profile.last().unwrap().known < 0.3, "{:?}", e.profile);
        let s = &e.stats;
        assert!(s.real >= 50 && s.fakes >= 6, "{s:?}");
        assert!(s.real_known <= s.real && s.fakes_caught <= s.fakes && s.checks_right <= s.checks);
    }
}
