import { describe, expect, it } from "vitest";
import type { DigestChapter, Diagrams } from "../ipc/digest";
import { diagramsHtml, textWidth, wrap } from "./diagrams";

const chapter = (idx: number, kind: DigestChapter["kind"], tension: number): DigestChapter => ({
  idx,
  title: `CHAPTER ${idx - 12} T`,
  summary: "",
  kind,
  events: [],
  characters: [],
  advice: "normal",
  adviceWhy: "",
  tension,
  quotes: [],
});
const chapters = Array.from({ length: 40 }, (_, i) => chapter(13 + i, i % 5 === 3 ? "digression" : "plot", (i % 5) + 1));
const parts = [
  { title: "岸上", from: 13, to: 30 },
  { title: "海上", from: 31, to: 52 },
];
const label = (idx: number) => `第 ${idx - 12} 章`;

const all: Diagrams = {
  chosen: ["relations", "plot", "tree", "family", "route", "rhythm", "compare", "causal", "concepts"].map((kind) => ({ kind, why: `why ${kind}` })),
  relations: {
    center: "ahab",
    groups: [
      { id: "cap", label: "船长" },
      { id: "mate", label: "大副" },
    ],
    people: [
      { id: "ahab", name: "亚哈", en: "Ahab", group: "cap" },
      { id: "starbuck", name: "斯达巴克", en: "Starbuck", group: "mate" },
      { id: "queequeg", name: "魁魁格", en: "", group: "x" },
      { id: "moby", name: "莫比·迪克<白>", en: "Moby Dick", group: "whale" },
    ],
    links: [
      { a: "ahab", b: "moby", kind: "conflict", label: "复仇" },
      { a: "ahab", b: "starbuck", kind: "conflict", label: "反对" },
      { a: "starbuck", b: "queequeg", kind: "rank", label: "鱼叉手" },
      { a: "ghost", b: "ahab", kind: "bond", label: "不存在的人" },
    ],
  },
  plot: {
    events: [
      { id: "e1", chapter: 14, text: "签约，听说船长只有一条腿" },
      { id: "e2", chapter: 20, text: "亚哈第一次登上后甲板" },
      { id: "e3", chapter: 40, text: "魁魁格造棺材" },
      { id: "e4", chapter: 52, text: "以实玛利抱着棺材获救" },
    ],
    links: [
      { a: "e1", b: "e2", kind: "cause", label: "一条腿" },
      { a: "e3", b: "e4", kind: "fate", label: "棺材救命" },
      { a: "e1", b: "e9", kind: "fate", label: "坏的" },
    ],
  },
  tree: {
    nodes: [
      { id: "owners", parent: "", name: "法勒", role: "船东", special: false },
      { id: "ahab", parent: "owners", name: "亚哈", role: "船长", special: false },
      { id: "fedallah", parent: "ahab", name: "费达拉", role: "私藏的艇队", special: true },
      { id: "starbuck", parent: "ahab", name: "斯达巴克", role: "大副", special: false },
    ],
  },
  // 有环：两个人互为上级，也不能死循环
  family: {
    nodes: [
      { id: "a", parent: "b", name: "甲", role: "", special: false },
      { id: "b", parent: "a", name: "乙", role: "", special: false },
    ],
  },
  route: {
    stops: [
      { place: "南塔克特", chapter: 22, what: "起航" },
      { place: "好望角", chapter: 30, what: "鬼喷" },
      { place: "太平洋", chapter: 52, what: "三天追逐，船沉" },
    ],
  },
  rhythm: {
    marks: [
      { chapter: 20, label: "立誓" },
      { chapter: 21, label: "紧挨着" },
      { chapter: 999, label: "不存在" },
    ],
  },
  compare: {
    title: "九次海上相遇",
    items: [
      { name: "信天翁号", en: "Goney", chapter: 30, badge: "没问成", tone: "bad", text: "号筒落海" },
      { name: "单身汉号", en: "Bachelor", chapter: 45, badge: "没见过", tone: "good", text: "满载鲸油回家" },
    ],
  },
  causal: {
    nodes: [
      { id: "c1", text: "捕鲸业扩张", kind: "cause" },
      { id: "c2", text: "抹香鲸减少", kind: "effect" },
      { id: "c3", text: "远洋航行", kind: "effect" },
    ],
    links: [
      { a: "c1", b: "c2", kind: "", label: "导致" },
      { a: "c2", b: "c3", kind: "", label: "迫使" },
      { a: "c3", b: "c1", kind: "", label: "环" },
    ],
  },
  concepts: {
    nodes: [
      { id: "w", text: "白色", kind: "concept" },
      { id: "t", text: "恐惧", kind: "concept" },
    ],
    links: [{ a: "w", b: "t", kind: "", label: "引起" }],
  },
};

describe("diagrams", () => {
  it("estimates and wraps Chinese and Latin text", () => {
    expect(textWidth("白鲸", 10)).toBe(20);
    expect(textWidth("ab", 10)).toBeCloseTo(11.2);
    expect(wrap("一二三四五六", 10, 30)).toEqual(["一二三", "四五六"]);
  });

  it("draws every chosen kind as SVG, in order, with a why line", () => {
    const html = diagramsHtml(all, chapters, parts, label);
    expect((html.match(/<svg /g) ?? []).length).toBe(9);
    expect(html).not.toMatch(/NaN|undefined|Infinity/);
    const titles = [...html.matchAll(/<h3>([^<]+)<\/h3>/g)].map((m) => m[1]);
    expect(titles).toEqual(["人物关系图", "故事进展与伏笔", "组织与等级", "家谱", "地点路线", "全书节奏", "九次海上相遇", "因果与论证", "概念关系"]);
    expect(html).toContain("why relations");
    // 转义；不存在的人、事件的连线丢掉
    expect(html).toContain("莫比·迪克&lt;白&gt;");
    expect(html).not.toContain("不存在的人");
    expect(html).not.toContain("坏的");
    expect(html).not.toContain("不存在");
    // 伏笔说明在图下面
    expect(html).toContain("棺材救命：第 28 章 → 第 40 章");
    // 私藏的关系画虚线
    expect(html).toMatch(/stroke:var\(--dg-fate\)" stroke-width="1.5" stroke-dasharray/);
  });

  it("skips chosen kinds without enough data", () => {
    const thin: Diagrams = { ...all, chosen: [{ kind: "route", why: "" }, { kind: "plot", why: "" }], route: { stops: [] }, plot: { events: [], links: [] } };
    expect(diagramsHtml(thin, chapters, parts, label)).toBe("");
  });
});
