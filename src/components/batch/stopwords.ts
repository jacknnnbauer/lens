// 从历史批量导出时默认不勾选的常见虚词（SPEC 12.1）。

const STOPWORDS = new Set(
  (
    "a an the and or but nor so yet for of to in on at by from with without into onto upon over under about above below " +
    "between among through during before after since until while as if than then that this these those there here " +
    "is am are was were be been being do does did done have has had having will would shall should can could may might must " +
    "i me my mine you your yours he him his she her hers it its we us our ours they them their theirs " +
    "not no yes all any some each every both either neither one other such own same very too also just only " +
    "what which who whom whose when where why how"
  ).split(" "),
);

export function isStopword(word: string): boolean {
  return STOPWORDS.has(word.trim().toLowerCase());
}
