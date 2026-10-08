// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { Drop } from "../../overlay/drop/Drop";
import { Bar } from "./Bar";
import { Keycap } from "./Keycap";
import { Segmented } from "./Segmented";
import { Switch } from "./Switch";

afterEach(cleanup);

describe("ui kit", () => {
  it("Segmented calls onChange with the value", () => {
    const onChange = vi.fn();
    render(
      <Segmented
        options={[
          { value: "Hold", label: "Удержание" },
          { value: "Toggle", label: "Нажатие" },
        ]}
        value="Hold"
        onChange={onChange}
      />,
    );
    fireEvent.click(screen.getByText("Нажатие"));
    expect(onChange).toHaveBeenCalledWith("Toggle");
    fireEvent.click(screen.getByText("Удержание"));
    expect(onChange).toHaveBeenCalledTimes(1);
    expect(screen.getByRole("radio", { checked: true }).textContent).toBe("Удержание");
  });

  it("Keycap reflects pressed state", () => {
    const { container, rerender } = render(<Keycap label="⌥" />);
    expect(container.querySelector("kbd")!.hasAttribute("data-pressed")).toBe(false);
    rerender(<Keycap label="⌥" pressed />);
    expect(container.querySelector("kbd")!.hasAttribute("data-pressed")).toBe(true);
  });

  it("Switch toggles", () => {
    const onChange = vi.fn();
    render(<Switch checked={false} onChange={onChange} label="x" />);
    fireEvent.click(screen.getByRole("switch"));
    expect(onChange).toHaveBeenCalledWith(true);
  });

  it("Bar clamps and warns near the limit", () => {
    const { container, rerender } = render(<Bar value={50} max={100} />);
    const fill = () => container.querySelector(".bar-fill") as HTMLElement;
    expect(fill().style.transform).toBe("scaleX(0.5)");
    rerender(<Bar value={500} max={100} />);
    expect(fill().style.transform).toBe("scaleX(1)");
    expect(fill().hasAttribute("data-accent")).toBe(true);
  });

  it("pill keeps its size and colours", () => {
    const { container } = render(<Drop mood="idle" level={0} translate={false} reduced />);
    const svg = container.querySelector("svg")!;
    expect(svg.getAttribute("width")).toBe("48");
    expect(svg.getAttribute("height")).toBe("50");
    expect(container.querySelector("[data-body] path")!.getAttribute("style")).toContain("var(--drop-body, #fff)");
    // The satellite and sparkles follow the body colour: white ones vanished on the light theme.
    for (const sel of ["[data-sat-front]", "[data-sat-back]", "[data-glint]"]) {
      expect(container.querySelector(sel)!.getAttribute("style")).toContain("var(--drop-body, #fff)");
    }
  });
});
