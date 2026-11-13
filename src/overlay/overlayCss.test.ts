import { describe, expect, it } from "vitest";
// `?raw` gives an empty string for this file under Vitest; read it from disk (Node runs the tests).
// @ts-ignore -- no Node types in the app's tsconfig
import { readFileSync } from "node:fs";

const css: string = readFileSync(new URL("./overlay.css", import.meta.url), "utf8");

const rule = (selector: string) => {
  const at = css.indexOf(`\n${selector} {`);
  return at < 0 ? "" : css.slice(at, css.indexOf("}", at));
};

describe("overlay.css", () => {
  it("island_row_lets_clicks_through", () => {
    // Live 2026-11-14 (the user's finding): the Drop's row (18–104 pt high, 30–390 pt wide) lay over
    // the agent card's buttons and the list's first row and swallowed their clicks.
    expect(rule(".island-row")).toMatch(/pointer-events:\s*none/);
  });
});
