// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from "vitest";
import { render } from "@testing-library/react";
import { DraftPanel } from "./DraftPanel";

afterEach(() => vi.restoreAllMocks());

const view = (c: HTMLElement) => c.querySelector("[data-draft-view]") as HTMLElement;

describe("DraftPanel", () => {
  it("renders settled text white and the revised piece grey, in italics", () => {
    const { container } = render(<DraftPanel draft={{ settled: "Привет, слушай", interim: "я посмотрела" }} />);
    const settled = container.querySelector("[data-settled]") as HTMLElement;
    const interim = container.querySelector("[data-interim]") as HTMLElement;
    expect(settled.textContent).toBe("Привет, слушай");
    expect(interim.textContent).toBe("я посмотрела");
    expect(settled.style.color).toBe("rgb(230, 232, 234)");
    expect(interim.style.color).toBe("rgb(138, 143, 150)");
    expect(view(container).style.fontStyle).toBe("italic");
  });

  it("is a framed field: #1C1C1E, 1 px #3A3A3D border, radius 14, no fade mask", () => {
    const { container } = render(<DraftPanel draft={{ settled: "a", interim: "" }} />);
    const field = container.querySelector("[data-draft-panel]") as HTMLElement;
    expect(field.style.background).toBe("rgb(28, 28, 30)");
    expect(field.style.border).toBe("1px solid rgb(58, 58, 61)");
    expect(field.style.borderRadius).toBe("14px");
    expect(field.style.height).toBe("128px");
    expect(field.style.willChange).toBe("opacity, transform");
    expect(field.outerHTML).not.toContain("mask");
  });

  it("text starts at the top of the field", () => {
    const { container } = render(<DraftPanel draft={{ settled: "a", interim: "b" }} />);
    expect(view(container).style.justifyContent).not.toBe("flex-end");
    expect(view(container).style.display).not.toBe("flex");
  });

  it("without a draft it lays out hidden italic sample text (font ready before the window opens)", () => {
    const { container } = render(<DraftPanel draft={null} shown={false} />);
    const field = container.querySelector("[data-draft-panel]") as HTMLElement;
    expect(field.hasAttribute("data-shown")).toBe(false);
    expect(view(container).textContent).not.toBe("");
    expect(container.querySelector("[data-settled]")).toBeNull();
  });

  it("long_draft_keeps_last_line_visible", () => {
    vi.spyOn(HTMLElement.prototype, "scrollHeight", "get").mockReturnValue(5400);
    vi.spyOn(HTMLElement.prototype, "clientHeight", "get").mockReturnValue(108);
    const long = Array.from({ length: 300 }, (_, i) => `строка ${i}`).join(" ");
    const { container, rerender } = render(<DraftPanel draft={{ settled: long, interim: "" }} />);
    expect(view(container).scrollTop).toBe(5400);
    view(container).scrollTop = 0;
    rerender(<DraftPanel draft={{ settled: long, interim: "ещё" }} />);
    expect(view(container).scrollTop).toBe(5400);
  });
});
