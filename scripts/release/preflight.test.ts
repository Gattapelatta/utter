import { describe, expect, it } from "vitest";
import { preflight, type Facts } from "./preflight";

const ok: Facts = {
  branch: "main",
  dirty: false,
  head: "abc",
  originMain: "abc",
  tagLocal: false,
  tagRemote: false,
  current: "0.1.0",
  next: "0.2.0",
  notes: "First public release.",
  identity: true,
  updaterKey: true,
  targets: ["aarch64-apple-darwin", "x86_64-apple-darwin"],
};

describe("preflight", () => {
  it("a clean main with everything in place may be released", () => {
    expect(preflight(ok)).toEqual([]);
  });

  it("preflight refuses an existing tag, local or remote", () => {
    expect(preflight({ ...ok, tagLocal: true }).join("\n")).toMatch(/v0\.2\.0/);
    const remote = preflight({ ...ok, tagRemote: true }).join("\n");
    expect(remote).toMatch(/v0\.2\.0/);
    expect(remote).toMatch(/gh release upload/);
  });

  it("preflight lists every problem in plain words", () => {
    const all = preflight({
      ...ok,
      branch: "feat/x",
      dirty: true,
      originMain: "def",
      next: "0.1.0",
      notes: "  ",
      identity: false,
      updaterKey: false,
      targets: ["aarch64-apple-darwin"],
    });
    expect(all).toHaveLength(8);
    for (const line of all) expect(line.length).toBeGreaterThan(10);
    expect(all.join("\n")).toMatch(/x86_64-apple-darwin/);
    expect(all.join("\n")).toMatch(/docs\/releases\/0\.1\.0\.md/);
  });

  it("missing notes are a problem too", () => {
    expect(preflight({ ...ok, notes: null })).toHaveLength(1);
  });
});
