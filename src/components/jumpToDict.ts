// 跳到查词结果里的某本词典（查词页上方的名称条、右侧栏的词典名称都用它）。

export function jumpToDict(id: string) {
  const el = document.getElementById(`dict-${id}`);
  el?.scrollIntoView({ block: "start", behavior: "smooth" });
  el?.querySelector<HTMLElement>("[data-card-toggle]")?.focus({ preventScroll: true });
}
