import { describe, expect, it } from "vitest";
import { cleanTitle, sourceText } from "./SourceLink";

describe("cleanTitle", () => {
  it("drops the browser, profile and extra-tab tails", () => {
    expect(cleanTitle("Opinion | Trump's Betrayal - The New York Times and 1 more page - Personal - Microsoft​ Edge")).toBe(
      "Opinion | Trump's Betrayal - The New York Times",
    );
    expect(cleanTitle("Some article - Google Chrome")).toBe("Some article");
    expect(cleanTitle("Doc and 3 more pages - Jack - Microsoft Edge")).toBe("Doc");
  });

  it("keeps ordinary titles", () => {
    expect(cleanTitle("report.docx - Word")).toBe("report.docx - Word");
    expect(cleanTitle("词镜 Lens")).toBe("词镜 Lens");
  });

  it("falls back to the host when there is no title", () => {
    expect(sourceText({ windowTitle: null, url: "https://www.nytimes.com/a" })).toBe("www.nytimes.com");
  });
});
