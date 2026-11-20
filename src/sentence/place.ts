// 把模型摘出的原文片段定位回原句（模型不给字符位置）：忽略大小写、弯直引号、多余空白；
// 按顺序从上一处之后找，找不到再从头找；实在没有就是 null（只在列表里显示，不标在原句上）。

/** 规整后的文字，以及每个字在原文里的位置 */
function normalize(s: string): { text: string; map: number[] } {
  let text = "";
  const map: number[] = [];
  let space = false;
  for (let i = 0; i < s.length; i++) {
    let c = s[i];
    if (/\s/.test(c)) {
      if (space || text === "") continue;
      space = true;
      c = " ";
    } else {
      space = false;
      c = c.replace(/[“”„″]/, '"').replace(/[‘’‚′]/, "'").toLowerCase();
    }
    text += c;
    map.push(i);
  }
  if (text.endsWith(" ")) {
    text = text.slice(0, -1);
    map.pop();
  }
  return { text, map };
}

export function placePieces(sentence: string, pieces: string[]): ([number, number] | null)[] {
  const s = normalize(sentence);
  let from = 0;
  return pieces.map((p) => {
    const q = normalize(p).text;
    if (!q) return null;
    let at = s.text.indexOf(q, from);
    if (at < 0) at = s.text.indexOf(q);
    if (at < 0) return null;
    from = at + q.length;
    return [s.map[at], s.map[at + q.length - 1] + 1];
  });
}
