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
  ghAuth: true,
};

describe("preflight", () => {
  it("a clean main with everything in place may be released", () => {
    expect(preflight(ok)).toEqual([]);
  });

  it("preflight refuses an existing tag, local or remote", () => {
    // Final review I2: the hints must lead somewhere — a local-only tag is removed; on GitHub a
    // missing release is created and a draft is completed and made public.
    const local = preflight({ ...ok, tagLocal: true }).join("\n");
    expect(local).toMatch(/v0\.2\.0/);
    expect(local).toMatch(/git tag -d v0\.2\.0/);
    const remote = preflight({ ...ok, tagLocal: true, tagRemote: true }).join("\n");
    expect(remote).toMatch(/gh release create v0\.2\.0/);
    expect(remote).toMatch(/gh release edit v0\.2\.0 --draft=false/);
    expect(remote).not.toMatch(/git tag -d/);
  });

  it("a GitHub login is checked before anything is built", () => {
    expect(preflight({ ...ok, ghAuth: false }).join("\n")).toMatch(/gh auth login/);
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
      ghAuth: false,
    });
    expect(all).toHaveLength(9);
    for (const line of all) expect(line.length).toBeGreaterThan(10);
    expect(all.join("\n")).toMatch(/x86_64-apple-darwin/);
    expect(all.join("\n")).toMatch(/docs\/releases\/0\.1\.0\.md/);
  });

  it("missing notes are a problem too", () => {
    expect(preflight({ ...ok, notes: null })).toHaveLength(1);
  });
});
