// 全新安装第一次打开时的引导：欢迎 → 测词汇量 → 看结果 → 试读。每一步都能跳过。

import { useState } from "react";
import { tr } from "../../i18n";
import { createArticle } from "../../ipc/articles";
import type { VocabEstimate } from "../../ipc/vocabTest";
import { useReader } from "../../stores/reader";
import { useRoute } from "../../stores/route";
import { useSettings } from "../../stores/settings";
import { toast } from "../../stores/toast";
import { nextStep, type Step, type StepEvent } from "./onboardingFlow";
import { SAMPLE_BODY, SAMPLE_TITLE } from "./sampleArticle";
import { VocabResult, VocabTest } from "./VocabTest";
import styles from "./Onboarding.module.css";

export function Onboarding({ onFinish }: { onFinish: () => void }) {
  const [step, setStep] = useState<Step>("welcome");
  const [result, setResult] = useState<VocabEstimate | null>(null);

  const finish = () => {
    useSettings.getState().update({ general: { introDone: true } });
    onFinish();
  };
  const advance = (event: StepEvent) => {
    const next = nextStep(step, event);
    if (next === "done") finish();
    else setStep(next);
  };
  const startReading = async () => {
    try {
      const id = await createArticle(SAMPLE_BODY, SAMPLE_TITLE);
      useReader.getState().open(id);
      useRoute.getState().go("reader");
      toast(tr("点文章里的任意一个词试试"));
    } catch (e) {
      toast(String(e));
    }
    finish();
  };

  return (
    <div className={styles.page} role="dialog" aria-modal="true" aria-label={tr("欢迎使用词镜 Lens")}>
      {step === "welcome" && (
        <div className={styles.panel}>
          <h1>{tr("欢迎使用词镜 Lens")}</h1>
          <p>{tr("离线查词、读英文原著、记生词，都在这里。内置词典已经装好，现在就能查词。")}</p>
          <p>{tr("先花三分钟测一下词汇量，读文章时就能按你的水平标出生词。")}</p>
          <div className={styles.actions}>
            <button type="button" className={styles.ghost} onClick={() => advance("skipAll")}>
              {tr("跳过引导")}
            </button>
            <button type="button" className={styles.primary} onClick={() => advance("continue")}>
              {tr("开始测试")}
            </button>
          </div>
        </div>
      )}
      {step === "test" && (
        <VocabTest
          onDone={(r) => {
            setResult(r);
            advance("tested");
          }}
          onCancel={() => advance("skipTest")}
        />
      )}
      {step === "result" && result && <VocabResult result={result} onClose={() => advance("continue")} closeLabel={tr("下一步")} />}
      {step === "read" && (
        <div className={styles.panel}>
          <h1>{tr("试读一段")}</h1>
          <p>{tr("接下来打开《白鲸》的开头，生词已经标好，点任意一个词就能查。这篇文章会留在文章库里，不需要时可以删除。")}</p>
          <div className={styles.actions}>
            <button type="button" className={styles.ghost} onClick={() => advance("skipAll")}>
              {tr("以后再说")}
            </button>
            <button type="button" className={styles.primary} onClick={() => void startReading()}>
              {tr("开始阅读")}
            </button>
          </div>
        </div>
      )}
    </div>
  );
}
