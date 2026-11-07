import { describe, expect, it } from "vitest";
import tokensCss from "./tokens.css?raw";
import { contrast } from "../lib/contrast";

type Tokens = Record<string, string>;

/** Reads `--name: #hex;` declarations from the first block whose selector contains `selector`. */
function block(selector: string): Tokens {
  const re = /([^{}]+)\{([^}]*)\}/g;
  for (const m of tokensCss.matchAll(re)) {
    if (!m[1].includes(selector)) continue;
    const out: Tokens = {};
    for (const d of m[2].matchAll(/--([\w-]+):\s*(#[0-9a-fA-F]{6})\s*;/g)) out[d[1]] = d[2].toUpperCase();
    return out;
  }
  throw new Error(`no block for ${selector}`);
}

const dark = block('[data-theme="dark"]');
const light = block('[data-theme="light"]');
const themes: [string, Tokens][] = [
  ["dark", dark],
  ["light", light],
];

describe("design tokens", () => {
  it("text and text-secondary reach 4.5:1 on bg and surface in both themes", () => {
    for (const [, t] of themes) {
      for (const fg of ["text", "text-secondary"]) {
        for (const bg of ["bg", "surface"]) {
          expect(contrast(t[fg], t[bg]), `${fg} on ${bg}`).toBeGreaterThanOrEqual(4.5);
        }
      }
    }
  });

  it("text reaches 4.5:1 on surface-raised in both themes", () => {
    for (const [, t] of themes) expect(contrast(t.text, t["surface-raised"])).toBeGreaterThanOrEqual(4.5);
  });

  it("pill accents reach 4.5:1 on #0A0A0A", () => {
    for (const a of ["accent-record", "accent-translate", "accent-success", "accent-danger", "accent-warning"]) {
      expect(contrast(dark[a], "#0A0A0A"), a).toBeGreaterThanOrEqual(4.5);
    }
  });

  it("dark tokens match spec §5.1", () => {
    expect(dark).toMatchObject({
      bg: "#0A0A0A",
      surface: "#141518",
      "surface-raised": "#1F2228",
      text: "#FFFFFF",
      "text-secondary": "#7D8187",
      "accent-record": "#4AD8EC",
      "accent-translate": "#9D79F7",
      "accent-success": "#2BB585",
      "accent-danger": "#F2555A",
      "accent-warning": "#F5B544",
    });
  });

  it("light tokens match spec §5.1", () => {
    expect(light).toMatchObject({
      bg: "#FAFAF9",
      surface: "#FFFFFF",
      "surface-raised": "#F2F2F0",
      text: "#0A0A0A",
      "text-secondary": "#5F636A",
      "accent-record": "#0E9FB5",
      "accent-translate": "#7448E8",
      "accent-success": "#1F8A65",
      "accent-danger": "#D93A40",
      "accent-warning": "#C98A12",
    });
  });
});
