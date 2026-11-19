// 词汇量测试：约 60 道「认识 / 不认识」，偶尔抽查释义。答案只留在这个组件里，中途退出什么都不保存。

import { useCallback, useEffect, useRef, useState } from "react";
import { tr } from "../../i18n";
import { vocabTestFinish, vocabTestNext, type VocabAnswer, type VocabEstimate, type VocabNext } from "../../ipc/vocabTest";
import { useSettings } from "../../stores/settings";
import { keyToAction, withAnswer, withCheck, type Action } from "./testFlow";
import { VocabResult } from "./VocabResult";
import styles from "./VocabTest.module.css";

export { VocabResult };

interface Props {
  onDone: (result: VocabEstimate) => void;
  onCancel: () => void;
}

export function VocabTest({ onDone, onCancel }: Props) {
  const [seed] = useState(() => Math.floor(Math.random() * 2 ** 31));
  const [answers, setAnswers] = useState<VocabAnswer[]>([]);
  const [question, setQuestion] = useState<VocabNext | null>(null);
  const [error, setError] = useState<string | null>(null);
  const finished = useRef(false);

  useEffect(() => {
    let alive = true;
    setQuestion(null);
    vocabTestNext(seed, answers).then(
      (next) => {
        if (!alive) return;
        if (next.kind !== "done") {
          setQuestion(next);
          return;
        }
        if (finished.current) return;
        finished.current = true;
        vocabTestFinish(seed, answers).then(
          async (result) => {
            await useSettings.getState().load();
            onDone(result);
          },
          (e) => setError(String(e)),
        );
      },
      (e) => alive && setError(String(e)),
    );
    return () => {
      alive = false;
    };
    // onDone 只在结束时用一次
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [seed, answers]);

  const act = useCallback(
    (action: Action) => {
      if (!question || question.kind === "done") return;
      if (question.kind === "word" && "known" in action) setAnswers((a) => withAnswer(a, question, action.known));
      if (question.kind === "check" && "option" in action) setAnswers((a) => withCheck(a, action.option, question.correct));
    },
    [question],
  );

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.repeat || !question || question.kind === "done") return;
      const action = keyToAction(e.key, question.kind);
      if (action) {
        e.preventDefault();
        act(action);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [question, act]);

  const index = question && question.kind !== "done" ? question.index : answers.length;
  const total = question && question.kind !== "done" ? question.total : 60;

  return (
    <div className={styles.test}>
      <div className={styles.top}>
        <div className={styles.bar} role="progressbar" aria-valuemin={0} aria-valuemax={total} aria-valuenow={index}>
          <span style={{ width: `${(index / total) * 100}%` }} />
        </div>
        <span className={styles.count}>
          {Math.min(index + 1, total)} / {total}
        </span>
        <button type="button" className={styles.link} onClick={onCancel}>
          {tr("退出测试")}
        </button>
      </div>

      {error && (
        <p className={styles.note} role="alert">
          {error}
        </p>
      )}

      {question?.kind === "word" && (
        <div className={styles.card}>
          <p className={styles.word} lang="en">
            {question.word}
          </p>
          <p className={styles.note}>{tr("凭第一感觉回答。拿不准的词选「不认识」，里面混了一些假词。")}</p>
          <div className={styles.choices}>
            <button type="button" className={styles.choice} onClick={() => act({ known: false })}>
              {tr("不认识")} <kbd>←</kbd>
            </button>
            <button type="button" className={`${styles.choice} ${styles.main}`} onClick={() => act({ known: true })}>
              {tr("认识")} <kbd>→</kbd>
            </button>
          </div>
        </div>
      )}

      {question?.kind === "check" && (
        <div className={styles.card}>
          <p className={styles.word} lang="en">
            {question.word}
          </p>
          <p className={styles.note}>{tr("这个词是什么意思？")}</p>
          <div className={styles.options}>
            {question.options.map((o, i) => (
              <button key={o} type="button" className={styles.choice} onClick={() => act({ option: i })}>
                <kbd>{i + 1}</kbd> {o}
              </button>
            ))}
            <button type="button" className={styles.choice} onClick={() => act({ option: null })}>
              <kbd>4</kbd> {tr("不确定")}
            </button>
          </div>
        </div>
      )}

      {!question && !error && <p className={styles.note}>{finished.current ? tr("正在计算结果…") : " "}</p>}
    </div>
  );
}

/** 盖住整个窗口的测试：从生词本进入时用 */
export function VocabTestOverlay({ onClose }: { onClose: () => void }) {
  const [result, setResult] = useState<VocabEstimate | null>(null);
  return (
    <div className={styles.overlay} role="dialog" aria-modal="true" aria-label={tr("词汇量测试")}>
      {result ? <VocabResult result={result} onClose={onClose} closeLabel={tr("完成")} /> : <VocabTest onDone={setResult} onCancel={onClose} />}
    </div>
  );
}
