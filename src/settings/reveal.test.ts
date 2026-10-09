// @vitest-environment jsdom
import { describe, expect, it, vi } from "vitest";
import { revealPage } from "./reveal";

function page(html: string) {
  const section = document.createElement("section");
  section.innerHTML = html;
  document.body.appendChild(section);
  const calls: { el: Element; delay: number; first: Keyframe }[] = [];
  Element.prototype.animate = vi.fn(function (this: Element, kf: Keyframe[], opts: KeyframeAnimationOptions) {
    calls.push({ el: this, delay: Number(opts.delay ?? 0), first: kf[0] });
    return {} as Animation;
  }) as unknown as typeof Element.prototype.animate;
  return { section, calls };
}

describe("section reveal", () => {
  it("the title rises from under its mask, then the blocks cascade in order", () => {
    const { section, calls } = page(`<div data-title><h1>Основное</h1></div><p>sub</p><div class="row">a</div><div class="row">b</div>`);
    revealPage(section);
    expect(calls[0].el.tagName).toBe("H1");
    expect(String(calls[0].first.transform)).toContain("translateY(");
    const rest = calls.slice(1);
    expect(rest.map((c) => c.el.textContent)).toEqual(["sub", "a", "b"]);
    const delays = rest.map((c) => c.delay);
    expect(delays).toEqual([...delays].sort((x, y) => x - y));
    expect(String(rest[0].first.filter)).toContain("blur");
  });

  it("a block holding the title is opened up, and long pages stop cascading early", () => {
    const rows = Array.from({ length: 30 }, (_, i) => `<li class="row">r${i}</li>`).join("");
    const { section, calls } = page(`<div><button data-drop-slot="about"></button><div data-title><h1>Это Капля</h1></div><p>slogan</p></div><ul>${rows}</ul>`);
    revealPage(section);
    const names = calls.map((c) => c.el.textContent);
    expect(names).toContain("slogan");
    expect(names).toContain("r0");
    expect(calls.filter((c) => c.el.tagName === "H1")).toHaveLength(1);
    expect(Math.max(...calls.map((c) => c.delay))).toBeLessThanOrEqual(500);
  });
});
