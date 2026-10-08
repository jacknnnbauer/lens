//! 间隔复习调度（M8）：FSRS-4.5 默认参数，目标记忆保持率 90%（此时间隔 = 稳定性 S 天）。
//! 学习 / 重学阶段的短期稳定性用 FSRS-5 的 S·e^{w17·(G−3+w18)}。纯函数，不碰数据库。

pub const DAY_MS: i64 = 24 * 3600 * 1000;
/// 忘了（或新词第一次就忘了）之后多久再出现
pub const RELEARN_MS: i64 = 10 * 60 * 1000;
const MAX_INTERVAL_DAYS: f64 = 36500.0;

const W: [f64; 19] = [
    0.4872, 1.4003, 3.7145, 13.8206, 5.1618, 1.2298, 0.8975, 0.031, 1.6474, 0.1367, 1.0461, 2.1072, 0.0793, 0.3246, 1.587,
    0.2272, 2.8755, 0.5425, 0.0912,
];
const DECAY: f64 = -0.5;
const FACTOR: f64 = 19.0 / 81.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum State {
    #[default]
    New = 0,
    Learning = 1,
    Review = 2,
    Relearning = 3,
}

impl State {
    pub fn from_i64(v: i64) -> Self {
        match v {
            1 => Self::Learning,
            2 => Self::Review,
            3 => Self::Relearning,
            _ => Self::New,
        }
    }
}

/// 1 忘了 · 2 模糊 · 3 记得 · 4 简单
pub type Rating = u8;

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Card {
    pub state: State,
    pub stability: f64,
    pub difficulty: f64,
    pub reps: i64,
    pub lapses: i64,
    pub last_review: Option<i64>,
    pub due: Option<i64>,
}

fn g(r: Rating) -> f64 {
    f64::from(r.clamp(1, 4))
}

fn init_stability(r: Rating) -> f64 {
    W[usize::from(r.clamp(1, 4)) - 1].max(0.1)
}

fn init_difficulty(r: Rating) -> f64 {
    (W[4] - (g(r) - 3.0) * W[5]).clamp(1.0, 10.0)
}

fn next_difficulty(d: f64, r: Rating) -> f64 {
    let d2 = d - W[6] * (g(r) - 3.0);
    (W[7] * init_difficulty(4) + (1.0 - W[7]) * d2).clamp(1.0, 10.0)
}

pub fn retrievability(elapsed_days: f64, s: f64) -> f64 {
    (1.0 + FACTOR * elapsed_days.max(0.0) / s.max(0.01)).powf(DECAY)
}

fn recall_stability(d: f64, s: f64, rv: f64, r: Rating) -> f64 {
    let hard = if r == 2 { W[15] } else { 1.0 };
    let easy = if r == 4 { W[16] } else { 1.0 };
    s * (1.0 + W[8].exp() * (11.0 - d) * s.powf(-W[9]) * ((W[10] * (1.0 - rv)).exp() - 1.0) * hard * easy)
}

fn forget_stability(d: f64, s: f64, rv: f64) -> f64 {
    (W[11] * d.powf(-W[12]) * ((s + 1.0).powf(W[13]) - 1.0) * (W[14] * (1.0 - rv)).exp()).min(s)
}

fn short_term_stability(s: f64, r: Rating) -> f64 {
    (s * (W[17] * (g(r) - 3.0 + W[18])).exp()).max(0.1)
}

fn interval_days(s: f64) -> i64 {
    s.round().clamp(1.0, MAX_INTERVAL_DAYS) as i64
}

/// 四种评分各自的结果（下标 0..4 对应评分 1..4）；间隔保证 模糊 ≤ 记得 < 简单。
pub fn preview(card: &Card, now: i64) -> [Card; 4] {
    let mut out = [Card::default(); 4];
    for r in 1..=4u8 {
        let mut c = *card;
        c.reps += 1;
        c.last_review = Some(now);
        match card.state {
            State::New => {
                c.stability = init_stability(r);
                c.difficulty = init_difficulty(r);
            }
            State::Learning | State::Relearning => {
                c.stability = short_term_stability(card.stability.max(0.1), r);
                c.difficulty = next_difficulty(card.difficulty, r);
            }
            State::Review => {
                let elapsed = card.last_review.map_or(0.0, |t| (now - t) as f64 / DAY_MS as f64);
                let rv = retrievability(elapsed, card.stability);
                c.stability = if r == 1 {
                    forget_stability(card.difficulty, card.stability, rv)
                } else {
                    recall_stability(card.difficulty, card.stability, rv, r)
                };
                c.difficulty = next_difficulty(card.difficulty, r);
            }
        }
        if r == 1 {
            if card.state == State::Review {
                c.lapses += 1;
                c.state = State::Relearning;
            } else if card.state == State::New {
                c.state = State::Learning;
            }
            c.due = Some(now + RELEARN_MS);
        } else {
            c.state = State::Review;
            c.due = Some(now + interval_days(c.stability) * DAY_MS);
        }
        out[usize::from(r) - 1] = c;
    }
    // 间隔单调：模糊不长于记得，简单至少比记得多一天
    let days = |c: &Card| c.due.map_or(0, |d| (d - now) / DAY_MS);
    let (h, gd, e) = (days(&out[1]), days(&out[2]), days(&out[3]));
    let h2 = h.min(gd);
    let g2 = gd.max(h2 + 1);
    let e2 = e.max(g2 + 1);
    out[1].due = Some(now + h2 * DAY_MS);
    out[2].due = Some(now + g2 * DAY_MS);
    out[3].due = Some(now + e2 * DAY_MS);
    out
}

pub fn schedule(card: &Card, rating: Rating, now: i64) -> Card {
    preview(card, now)[usize::from(rating.clamp(1, 4)) - 1]
}

/// 熟悉度（0..5）由复习状态推出：新词 0，学习中 1，之后按稳定性分档，≥ 90 天算已掌握。
pub fn familiarity(c: &Card) -> i64 {
    match c.state {
        State::New => 0,
        State::Learning | State::Relearning => 1,
        State::Review if c.stability < 7.0 => 2,
        State::Review if c.stability < 30.0 => 3,
        State::Review if c.stability < 90.0 => 4,
        State::Review => 5,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const T0: i64 = 1_700_000_000_000;

    #[test]
    fn new_card_intervals() {
        let p = preview(&Card::default(), T0);
        assert_eq!(p[0].due, Some(T0 + RELEARN_MS));
        assert_eq!(p[0].state, State::Learning);
        let days: Vec<i64> = p[1..].iter().map(|c| (c.due.unwrap_or(0) - T0) / DAY_MS).collect();
        assert_eq!(days, vec![1, 4, 14], "FSRS-4.5 初始稳定性 1.40 / 3.71 / 13.82 天");
        assert!(p.iter().all(|c| c.reps == 1 && c.last_review == Some(T0)));
    }

    #[test]
    fn review_grows_and_lapse_shrinks() {
        let c = schedule(&Card::default(), 3, T0);
        let due = c.due.unwrap_or(0);
        let again = schedule(&c, 3, due);
        assert!(again.stability > c.stability * 2.0, "按时记得，稳定性明显增长：{}", again.stability);
        assert!(again.difficulty <= c.difficulty + 1e-9);
        let lapse = schedule(&again, 1, again.due.unwrap_or(0));
        assert_eq!((lapse.state, lapse.lapses), (State::Relearning, 1));
        assert!(lapse.stability < again.stability);
        let p = preview(&again, again.due.unwrap_or(0));
        let d: Vec<i64> = p.iter().map(|x| x.due.unwrap_or(0)).collect();
        assert!(d[0] < d[1] && d[1] < d[2] && d[2] < d[3], "间隔单调：{d:?}");
    }

    #[test]
    fn relearning_graduates() {
        let c = schedule(&Card::default(), 1, T0);
        let back = schedule(&c, 3, T0 + RELEARN_MS);
        assert_eq!(back.state, State::Review);
        assert!(back.due.unwrap_or(0) - (T0 + RELEARN_MS) >= DAY_MS);
        assert_eq!(familiarity(&back), 2);
        assert_eq!(familiarity(&c), 1);
    }

    #[test]
    fn retrievability_is_90_percent_at_stability() {
        assert!((retrievability(10.0, 10.0) - 0.9).abs() < 1e-9);
        assert!((retrievability(0.0, 3.0) - 1.0).abs() < 1e-12);
    }
}
