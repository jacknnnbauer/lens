// 语境卡 / 复习卡背面的释义：第一本命中的本地词典的完整词条（只查本地，不影响当前查询）。

import { useEffect, useState } from "react";
import { DictBody } from "../dict-render/DictBody";
import { prepareEntry } from "../dict-render/prepare";
import { tr } from "../i18n";
import { firstEntry } from "../ipc/lookup";
import { useSettings } from "../stores/settings";
import styles from "./CardDefinition.module.css";

interface Shown {
  dictId: string;
  name: string;
  html: string[];
}

export function CardDefinition({ word, onLookup }: { word: string; onLookup: (w: string) => void }) {
  const [shown, setShown] = useState<Shown | null | "loading">("loading");
  // 换了背面词典就重新查
  const cardDict = useSettings((s) => s.settings?.wordbook?.cardDict ?? "");

  useEffect(() => {
    let alive = true;
    setShown("loading");
    firstEntry(word).then(
      (r) => alive && setShown(r && { dictId: r.dictId, name: r.name, html: r.entries.map((e) => prepareEntry(e.body, r.dictId).html) }),
      () => alive && setShown(null),
    );
    return () => {
      alive = false;
    };
  }, [word, cardDict]);

  if (shown === "loading") return <p className={styles.note}>{tr("查询中…")}</p>;
  if (!shown) return <p className={styles.note}>{tr("本地词典里没有找到释义")}</p>;
  return (
    <section className={styles.box} aria-label={tr("释义")}>
      <h3 className={styles.name}>{shown.name}</h3>
      <div className={styles.body}>
        {shown.html.map((h, i) => (
          <DictBody key={i} html={h} onEntry={onLookup} dictId={shown.dictId} />
        ))}
      </div>
    </section>
  );
}
