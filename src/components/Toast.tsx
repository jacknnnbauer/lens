import { useToast } from "../stores/toast";
import styles from "./Toast.module.css";

/** 屏幕阅读器通过 role=status 播报；容器常驻，内容变化才会被读出 */
export function Toast() {
  const { text, note } = useToast();
  return (
    <div role="status" aria-live="polite" className={styles.wrap}>
      {text && (
        <div className={styles.toast}>
          {text}
          {note && <span className={styles.note}>· {note}</span>}
        </div>
      )}
    </div>
  );
}
