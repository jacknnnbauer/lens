// 复习、标记认识之后更新今天的词汇量，并把后端改过的阅读门槛同步回界面。

import { vocabRefresh } from "../ipc/vocabTest";
import { useSettings } from "./settings";

export async function syncVocab(): Promise<void> {
  try {
    if ((await vocabRefresh()) != null) await useSettings.getState().load();
  } catch {
    // 只影响成长曲线的更新时机，下次操作会补上
  }
}
