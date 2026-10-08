import { describe, expect, it } from "vitest";
import { createModel, stepModel, type ModelInput } from "./dropModel";

const listening = (level: number): ModelInput => ({ mood: "listening", level, translate: false, reduced: false });

/** Largest frame-to-frame change of the head's angular velocity (deg per frame²). */
function headJerk(levels: (i: number) => number, frames = 240): number {
  const m = createModel(0);
  let prevRot = 0;
  let prevVel = 0;
  let worst = 0;
  for (let i = 1; i <= frames; i++) {
    const f = stepModel(m, listening(levels(i)), (i * 1000) / 60);
    const vel = f.rot - prevRot;
    if (i > 30) worst = Math.max(worst, Math.abs(vel - prevVel));
    prevRot = f.rot;
    prevVel = vel;
  }
  return worst;
}

describe("drop model", () => {
  it("head stays smooth when the microphone level jumps every frame", () => {
    // Real speech: the level flips between silence and loud syllables at 30 Hz.
    const spiky = headJerk((i) => (i % 2 ? 1 : 0));
    const steady = headJerk(() => 0.5);
    expect(spiky).toBeLessThan(steady * 2 + 0.002);
  });

  it("head does not bob up and down with syllables", () => {
    const m = createModel(0);
    const ys: number[] = [];
    for (let i = 1; i <= 240; i++) ys.push(stepModel(m, listening(i % 6 < 3 ? 1 : 0), (i * 1000) / 60).y);
    const tail = ys.slice(120);
    expect(Math.max(...tail) - Math.min(...tail)).toBeLessThan(0.6);
  });

  it("eyes close while listening and smile when happy", () => {
    const m = createModel(0);
    let f = stepModel(m, listening(0.5), 0);
    for (let i = 1; i <= 90; i++) f = stepModel(m, listening(0.5), (i * 1000) / 60);
    expect(f.lid).toBeGreaterThan(0.95);
    for (let i = 91; i <= 180; i++) f = stepModel(m, { ...listening(0), mood: "happy" }, (i * 1000) / 60);
    expect(f.smile).toBeGreaterThan(0.9);
  });
});
