// @vitest-environment jsdom
import { describe, expect, it } from "vitest";
import { act, render } from "@testing-library/react";
import { AnimatePresence, motion } from "motion/react";
import { islandVariants } from "./islandMotion";

const notch = { width: 179, height: 32 };
const scaleX = (el: HTMLElement) => {
  const m = el.style.transform.match(/scaleX\(([\d.e-]+)\)/);
  return m ? Number(m[1]) : el.style.transform === "none" || el.style.transform === "" ? 1 : NaN;
};
const wait = (ms: number) => act(() => new Promise((r) => setTimeout(r, ms)));

function Shell({ visible }: { visible: boolean }) {
  return (
    <AnimatePresence custom={false}>
      {visible && (
        <motion.div key="island" data-testid="island" variants={islandVariants(notch)} initial="hidden" animate="visible" exit="leave" />
      )}
    </AnimatePresence>
  );
}

describe("island exit", () => {
  it("folds from full size down to the notch, never collapsing to nothing first", async () => {
    const { getByTestId, rerender } = render(<Shell visible />);
    await wait(900); // grown and settled
    const el = getByTestId("island");
    expect(scaleX(el)).toBeCloseTo(1, 2);
    rerender(<Shell visible={false} />);
    const samples: number[] = [];
    for (let i = 0; i < 4; i++) {
      await wait(80);
      if (el.isConnected) samples.push(scaleX(el));
    }
    expect(samples.length).toBeGreaterThan(0);
    // The fold starts at full width and only shrinks towards the notch (179 / 360 ≈ 0.497).
    for (const s of samples) expect(s, `samples ${samples.join(", ")}`).toBeGreaterThan(0.49);
    expect(samples[0]).toBeGreaterThan(0.8);
  });
});
