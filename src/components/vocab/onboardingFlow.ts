// 新手引导的步骤。

export type Step = "welcome" | "test" | "result" | "read";
export type StepEvent = "continue" | "tested" | "skipTest" | "skipAll";

export function nextStep(step: Step, event: StepEvent): Step | "done" {
  if (event === "skipAll") return "done";
  if (step === "welcome") return "test";
  if (step === "test") return event === "tested" ? "result" : "read";
  if (step === "result") return "read";
  return "done";
}
