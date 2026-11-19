// 词汇量结果的解读：大致相当于哪个阶段、能读什么、下一步怎么做（不依赖界面，单独测试）。
// 参照点是各类考纲 / 课标要求的词数，口径和本测试（原形词数、按词频排名）不完全一样，只作大致参照。

export interface Milestone {
  /** 参照点的词数 */
  at: number;
  /** 刻度下方的短名（两个字，排得下） */
  short: string;
  /** 悬停时的全称 */
  label: string;
}

/** 刻度上的参照点 */
export const MILESTONES: Milestone[] = [
  { at: 1600, short: "初中", label: "初中课标" },
  { at: 3000, short: "高中", label: "高中课标" },
  { at: 4500, short: "四级", label: "大学英语四级" },
  { at: 6000, short: "六级", label: "大学英语六级" },
  { at: 8000, short: "托福", label: "雅思 / 托福高分" },
  { at: 13000, short: "专八", label: "专八 / GRE" },
  { at: 20000, short: "母语", label: "受过教育的母语者" },
];

export interface LevelInfo {
  /** 阶段名，例如「六级水平」 */
  title: string;
  /** 一句话评价 */
  summary: string;
  /** 现在能比较轻松读的东西 */
  canRead: string;
  /** 下一步建议 */
  next: string;
}

interface Tier extends LevelInfo {
  /** 达到这个词数就算这一档 */
  from: number;
}

const TIERS: Tier[] = [
  {
    from: 0,
    title: "入门阶段",
    summary: "最常用的词还在积累中，先把高频词打牢，进步会非常快。",
    canRead: "简单的分级读物、配图的儿童读物",
    next: "每天读一点分级读物，遇到的生词全部加入生词本复习",
  },
  {
    from: 1600,
    title: "大致相当于初中水平",
    summary: "日常最常用的词基本掌握，能看懂简单句子和短文。",
    canRead: "低阶分级读物、简单的英文对话",
    next: "多读分级读物，把 3,000 以内的常用词补齐",
  },
  {
    from: 3000,
    title: "大致相当于高中水平",
    summary: "常用词已经比较扎实，读一般文章时生词开始集中在正式用语上。",
    canRead: "中阶分级读物、青少年小说、简单的新闻",
    next: "开始读青少年小说，留意学术和书面常用词",
  },
  {
    from: 4500,
    title: "大致相当于四级水平",
    summary: "能应付大多数日常阅读，但读原版书时每页还会碰到不少生词。",
    canRead: "青少年小说、英文网站和通俗文章",
    next: "挑一本喜欢题材的原版小说，边读边查，坚持读完",
  },
  {
    from: 6000,
    title: "大致相当于六级、考研水平",
    summary: "已经可以读原版书了，生词主要是不太常见的描写和专业用词。",
    canRead: "通俗原版小说、大众杂志、一般新闻报道",
    next: "扩大阅读题材，多读非虚构和新闻评论",
  },
  {
    from: 8000,
    title: "大致相当于雅思、托福高分水平",
    summary: "阅读面已经很宽，大多数原版书可以比较顺畅地读下来。",
    canRead: "多数原版小说、《经济学人》一类的报刊、学术普及读物",
    next: "读文学作品和专业领域的书，积累低频但地道的词",
  },
  {
    from: 13000,
    title: "大致相当于专八、GRE 水平",
    summary: "词汇量已经超过绝大多数英语学习者，文学和学术文本都能读。",
    canRead: "文学经典、学术文章、各类报刊",
    next: "关注词的搭配和语气，从「认识」走向「会用」",
  },
  {
    from: 20000,
    title: "接近英语母语者",
    summary: "这已经是受过教育的母语者的词汇量范围。",
    canRead: "几乎所有类型的英文读物",
    next: "保持阅读，生词本里留下的就是真正的冷僻词",
  },
];

/** 这个词汇量的解读 */
export function levelInfo(value: number): LevelInfo {
  let tier = TIERS[0];
  for (const t of TIERS) if (value >= t.from) tier = t;
  const { from: _from, ...info } = tier;
  return info;
}

/** 刻度位置（0–1）：按平方根排，常用词那头不挤，高端也不会拉得太长 */
export function scalePos(value: number, min = 500, max = 25000): number {
  const v = Math.min(max, Math.max(min, value));
  return (Math.sqrt(v) - Math.sqrt(min)) / (Math.sqrt(max) - Math.sqrt(min));
}

// ---------- 学习者参照曲线（估算） ----------
// 没有可信的公开统计可用：这里假设中国英语学习者的词汇量呈对数正态分布，
// 中位数约 4,000（大学四级附近），σ = 0.75（约 10% 在 1,500 以下、10% 在 10,500 以上）。只作大致参照。
export const LEARNER_MEDIAN = 4000;
export const LEARNER_SIGMA = 0.75;

/** 标准正态分布函数（Abramowitz–Stegun 7.1.26，误差 < 1.5e-7） */
function phi(z: number): number {
  const t = 1 / (1 + 0.3275911 * (Math.abs(z) / Math.SQRT2));
  const y = 1 - ((((1.061405429 * t - 1.453152027) * t + 1.421413741) * t - 0.284496736) * t + 0.254829592) * t * Math.exp(-(z * z) / 2);
  return z >= 0 ? (1 + y) / 2 : (1 - y) / 2;
}

/** 这个词汇量超过了百分之多少的学习者（0–100，估算） */
export function learnerPercentile(value: number): number {
  if (value <= 0) return 0;
  return 100 * phi((Math.log(value) - Math.log(LEARNER_MEDIAN)) / LEARNER_SIGMA);
}

/** 显示用：「超过约 63%」；两头收在 1% 和 99% */
export function percentileText(value: number): number {
  return Math.min(99, Math.max(1, Math.round(learnerPercentile(value))));
}

/** 画在刻度上的曲线：n 个点 [横向位置 0–1, 高度 0–1]，横向和 scalePos 用同一把尺 */
export function learnerCurve(n = 120, min = 500, max = 25000): [number, number][] {
  const inv = (p: number) => (Math.sqrt(min) + p * (Math.sqrt(max) - Math.sqrt(min))) ** 2;
  const ys: number[] = [];
  for (let i = 0; i < n; i++) {
    // 每一小格里的人数比例 = 两端的分布函数之差
    const a = learnerPercentile(inv(i / n));
    const b = learnerPercentile(inv((i + 1) / n));
    ys.push(b - a);
  }
  const top = Math.max(...ys);
  return ys.map((y, i) => [(i + 0.5) / n, y / top]);
}
