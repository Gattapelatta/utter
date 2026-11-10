import { describe, expect, it } from "vitest";
import { createModel, gazeToward, stepModel, type Frame, type ModelInput } from "./dropModel";

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

  it("gaze overrides looking around", () => {
    const m = createModel(0, () => 0); // random() = 0 would always pick the first idle look (0, 0)
    const idle: ModelInput = { mood: "idle", level: 0, translate: false, reduced: false, gaze: { x: 8, y: -2 } };
    let f = stepModel(m, idle, 0);
    for (let i = 1; i <= 240; i++) f = stepModel(m, idle, (i * 1000) / 60);
    expect(f.ex).toBeCloseTo(8, 1);
    expect(f.ey).toBeCloseTo(-2, 1);
  });

  it("gazeToward points at the cursor and stays in the eye range", () => {
    const right = gazeToward(500, 0);
    expect(right.x).toBeCloseTo(8, 5);
    expect(right.y).toBeCloseTo(0, 5);
    const below = gazeToward(0, 500);
    expect(below.y).toBeLessThanOrEqual(3);
    expect(gazeToward(0, -500).y).toBeCloseTo(-5.5, 5);
    // Right next to the character the eyes stay near the centre instead of rolling to the side.
    expect(Math.abs(gazeToward(12, 0).x)).toBeLessThan(1);
  });
});

/** A small deterministic random source. */
function seeded(seed = 7) {
  let s = seed;
  return () => {
    s = (s * 1103515245 + 12345) % 2147483648;
    return s / 2147483648;
  };
}

const rest = (extra: Partial<ModelInput> = {}): ModelInput => ({ mood: "idle", level: 0, translate: false, reduced: false, rest: true, ...extra });

/** Runs the model at 60 fps for `seconds`, calling `each` with every frame. */
function run(m: ReturnType<typeof createModel>, input: ModelInput, from: number, seconds: number, each: (f: Frame, now: number) => void) {
  for (let i = 1; i <= seconds * 60; i++) {
    const now = from + (i * 1000) / 60;
    each(stepModel(m, input, now), now);
  }
}

describe("drop in the notch (rest)", () => {
  it("looks around every 2 to 5 s", () => {
    const m = createModel(0, seeded());
    const changes: number[] = [];
    let prev = "";
    run(m, rest(), 0, 120, (_f, now) => {
      const target = `${m.P.ex.t},${m.P.ey.t}`;
      if (target !== prev) changes.push(now);
      prev = target;
    });
    const gaps = changes.slice(2).map((t, i) => t - changes[i + 1]);
    expect(gaps.length).toBeGreaterThan(10);
    // A new look may repeat the old target, so a gap can span two looks; never shorter than 2 s.
    for (const g of gaps) expect(g).toBeGreaterThanOrEqual(2000 - 20);
    expect(Math.min(...gaps)).toBeLessThanOrEqual(5000 + 20);
  });

  it("smiles every 30 to 60 s for about 1.6 s", () => {
    const m = createModel(0, seeded(3));
    const starts: number[] = [];
    let smiling = false;
    let longest = 0;
    let since = 0;
    run(m, rest(), 0, 300, (f, now) => {
      if (f.smile > 0.5 && !smiling) {
        starts.push(now);
        since = now;
      }
      if (f.smile <= 0.5 && smiling) longest = Math.max(longest, now - since);
      smiling = f.smile > 0.5;
    });
    expect(starts.length).toBeGreaterThanOrEqual(5);
    expect(starts.length).toBeLessThanOrEqual(10);
    expect(longest).toBeLessThan(2200); // 1.6 s held + the spring's way up and down
    for (let i = 1; i < starts.length; i++) expect(starts[i] - starts[i - 1]).toBeGreaterThanOrEqual(30000 - 50);
  });

  it("has no glow", () => {
    const m = createModel(0, seeded());
    let f!: Frame;
    run(m, rest(), 0, 3, (x) => (f = x));
    expect(f.glow).toBeLessThan(0.01);
  });

  it("settles and reports when it must wake next", () => {
    const m = createModel(0, seeded());
    let settledAt = -1;
    let frame!: Frame;
    run(m, rest(), 0, 60, (f, now) => {
      if (settledAt < 0 && f.settled) {
        settledAt = now;
        frame = f;
      }
    });
    expect(settledAt).toBeGreaterThan(0);
    const pending = [m.nextLook, m.nextBlink, m.nextSmile].filter((t) => t > settledAt);
    expect(frame.wakeAt).toBeGreaterThan(settledAt);
    expect(frame.wakeAt).toBeLessThanOrEqual(Math.min(...pending) + 1);
  });

  it("never settles while listening", () => {
    const m = createModel(0, seeded());
    run(m, { mood: "listening", level: 0, translate: false, reduced: false }, 0, 10, (f) => expect(f.settled).toBe(false));
  });

  it("with reduced motion never smiles or looks around", () => {
    const m = createModel(0, seeded());
    run(m, rest({ reduced: true }), 0, 300, (f) => {
      expect(f.smile).toBeLessThan(0.01);
      expect(m.P.ex.t).toBe(0);
      expect(m.P.ey.t).toBe(0);
    });
  });

  it("outside the notch keeps its usual glow", () => {
    const m = createModel(0, seeded());
    let f!: Frame;
    run(m, rest({ rest: false }), 0, 3, (x) => (f = x));
    expect(f.glow).toBeGreaterThan(0.7);
  });
});

describe("dizzy (easter egg, spec 2026-11-09 §3.6)", () => {
  const dizzy: ModelInput = { mood: "dizzy", level: 0, translate: false, reduced: false };

  it("sways ±6° at 2 Hz with spiral eyes and blush on", () => {
    const m = createModel(0);
    let max = 0;
    let crossings = 0;
    let prev = 0;
    let last: Frame | null = null;
    for (let i = 1; i <= 120; i++) {
      const f = stepModel(m, dizzy, (i * 1000) / 60);
      if (i > 30) {
        max = Math.max(max, Math.abs(f.rot));
        if (Math.sign(f.rot) !== Math.sign(prev) && prev !== 0) crossings++;
      }
      prev = f.rot;
      last = f;
    }
    // 1.5 s at 2 Hz: ~6 zero crossings; the spring reaches most of the ±6°.
    expect(max).toBeGreaterThan(4);
    expect(max).toBeLessThanOrEqual(6.5);
    expect(crossings).toBeGreaterThanOrEqual(5);
    expect(crossings).toBeLessThanOrEqual(7);
    expect(last!.dizzy).toBeGreaterThan(0.95);
    expect(last!.settled).toBe(false);
  });

  it("other moods have no spirals", () => {
    const m = createModel(0);
    const f = stepModel(m, { mood: "idle", level: 0, translate: false, reduced: false }, 16);
    expect(f.dizzy).toBe(0);
  });
});

describe("tickle (each poke, the user 2026-11-10)", () => {
  it("smiles and wiggles for a moment, then settles again", () => {
    const m = createModel(0);
    const idle: ModelInput = { mood: "idle", level: 0, translate: false, reduced: false };
    for (let i = 1; i <= 60; i++) stepModel(m, idle, (i * 1000) / 60);
    const at = 1000;
    let smile = 0;
    let rot = 0;
    for (let i = 1; i <= 30; i++) {
      const f = stepModel(m, { ...idle, tickle: at }, at + (i * 1000) / 60);
      smile = Math.max(smile, f.smile);
      rot = Math.max(rot, Math.abs(f.rot));
      expect(f.settled).toBe(false);
    }
    expect(smile).toBeGreaterThan(0.5);
    expect(rot).toBeGreaterThan(1.5);
    let last: Frame | null = null;
    for (let i = 31; i <= 240; i++) last = stepModel(m, { ...idle, tickle: at }, at + (i * 1000) / 60);
    expect(last!.smile).toBeLessThan(0.05);
    expect(Math.abs(last!.rot)).toBeLessThan(0.05);
  });
});
