import { describe, expect, it } from "vitest";
import { nextStep } from "./onboardingFlow";

describe("新手引导的步骤", () => {
  it("欢迎 → 测试 → 结果 → 试读 → 完成", () => {
    expect(nextStep("welcome", "continue")).toBe("test");
    expect(nextStep("test", "tested")).toBe("result");
    expect(nextStep("result", "continue")).toBe("read");
    expect(nextStep("read", "continue")).toBe("done");
  });
  it("跳过测试直接到试读", () => {
    expect(nextStep("test", "skipTest")).toBe("read");
  });
  it("任何一步都能结束引导", () => {
    for (const step of ["welcome", "test", "result", "read"] as const) expect(nextStep(step, "skipAll")).toBe("done");
  });
});
