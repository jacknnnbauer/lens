//! 词汇量测试和成长曲线的命令。出题、估算在 `crate::vocab_test`，数据在 `crate::db::vocab`。

use tauri::State;

use crate::app_state::AppState;
use crate::db::now_ms;
use crate::db::vocab::{self, NewTest, Summary};
use crate::error::Result;
use crate::settings::Vocab;
use crate::vocab_test::{self, Answer, Estimate, Next};

/// 重测时避开最近两次考过的词
const AVOID_TESTS: usize = 2;

/// 排名用内置词频表：和测试是同一把尺子
fn rank(word: &str) -> Option<u32> {
    crate::freq::get().rank(word)
}

/// 词汇量变了：跟随时阅读门槛一起变，手动选过门槛的不动
fn apply_result(v: &Vocab, value: u32) -> Vocab {
    Vocab { level: if v.follow { value } else { v.level }, follow: v.follow, estimate: value }
}

fn store(state: &AppState, value: u32) -> Result<()> {
    let cur = state.settings.get()?.vocab;
    let next = apply_result(&cur, value);
    if next != cur {
        state.settings.patch(serde_json::json!({ "vocab": next }))?;
    }
    Ok(())
}

/// 下一题：同样的种子和作答记录总是得到同一题
#[tauri::command]
pub async fn vocab_test_next(state: State<'_, AppState>, seed: u64, answers: Vec<Answer>) -> Result<Next> {
    let avoid = state.db.with_conn(|c| vocab::recent_asked(c, AVOID_TESTS))?;
    Ok(vocab_test::next(seed, &answers, &avoid))
}

/// 答完了：算结果、存记录、更新阅读门槛。中途退出不会调用它，什么都不留下
#[tauri::command]
pub async fn vocab_test_finish(state: State<'_, AppState>, seed: u64, answers: Vec<Answer>, day: String) -> Result<Estimate> {
    let e = vocab_test::estimate(&answers, seed);
    let said_known: Vec<String> = answers.iter().filter(|a| a.band.is_some() && a.said_known).map(|a| a.word.clone()).collect();
    let asked: Vec<String> = answers.iter().map(|a| a.word.clone()).collect();
    let current = state.db.with_conn(|c| {
        vocab::insert_test(
            c,
            &NewTest { taken_at: now_ms(), day: &day, value: e.value, low: e.low, high: e.high, bank_version: vocab_test::bank_version(), said_known: &said_known, asked: &asked },
        )?;
        vocab::refresh(c, &day, &rank)
    })?;
    store(&state, current.unwrap_or(e.value))?;
    Ok(e)
}

/// 复习、标记认识之后调用：更新今天的词汇量。还没测过时返回 None
#[tauri::command]
pub async fn vocab_refresh(state: State<'_, AppState>, day: String) -> Result<Option<u32>> {
    let current = state.db.with_conn(|c| vocab::refresh(c, &day, &rank))?;
    if let Some(v) = current {
        store(&state, v)?;
    }
    Ok(current)
}

/// 成长曲线的数据
#[tauri::command]
pub async fn vocab_summary(state: State<'_, AppState>, month_start_day: String) -> Result<Option<Summary>> {
    state.db.with_conn(|c| vocab::summary(c, &month_start_day))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn result_sets_level_when_following() {
        let v = apply_result(&Vocab { level: 0, follow: true, estimate: 0 }, 7800);
        assert_eq!((v.level, v.estimate), (7800, 7800));
    }

    #[test]
    fn result_keeps_manual_level() {
        let v = apply_result(&Vocab { level: 8000, follow: false, estimate: 0 }, 7800);
        assert_eq!((v.level, v.follow, v.estimate), (8000, false, 7800));
    }
}
