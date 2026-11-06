// 「也可能是某词的变形」：比较词头和原形时用，规则与后端 normalize_key 一致（NFC、去首尾空白、小写）

export function normalizeHeadword(s: string): string {
  return s.normalize("NFC").trim().toLowerCase();
}
