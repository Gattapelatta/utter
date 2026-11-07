import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { archiveProblems, listTarGz } from "./archive";

const fx = (name: string) => readFileSync(new URL(`./fixtures/${name}`, import.meta.url));

// Final review C1 (2026-11-07): macOS tar added AppleDouble "._*" entries; the updater strips the
// first path component, so "._Utter.app" became an empty path and every update failed to install.
describe("update archive", () => {
  it("lists entry names without macOS tar's help", () => {
    expect(listTarGz(fx("clean.tar.gz"))).toEqual([
      "Utter.app/",
      "Utter.app/Contents/",
      "Utter.app/Contents/MacOS/",
      "Utter.app/Contents/Info.plist",
      "Utter.app/Contents/MacOS/utter",
    ]);
  });

  it("an archive made by macOS tar with metadata is refused", () => {
    const problems = archiveProblems(listTarGz(fx("apple-metadata.tar.gz")));
    expect(problems.join("\n")).toMatch(/\._Utter\.app/);
    expect(problems.join("\n")).toMatch(/\._utter/);
  });

  it("a clean archive with only Utter.app passes", () => {
    expect(archiveProblems(listTarGz(fx("clean.tar.gz")))).toEqual([]);
  });

  it("anything outside Utter.app is refused", () => {
    expect(archiveProblems(["Utter.app/", "Other.app/x"])).toHaveLength(1);
    expect(archiveProblems([])).toHaveLength(1);
  });
});
