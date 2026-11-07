import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { verifyMinisign } from "./minisign";

const fx = (name: string) => readFileSync(new URL(`./fixtures/${name}`, import.meta.url));
const payload = fx("payload.bin");
const sig = fx("payload.bin.sig").toString("utf8");
const pubA = fx("test-a.key.pub").toString("utf8");
const pubB = fx("test-b.key.pub").toString("utf8");

describe("minisign", () => {
  it("verifyMinisign accepts a real Tauri signature", () => {
    expect(verifyMinisign(payload, sig, pubA)).toBe(true);
  });

  it("verifyMinisign rejects a changed file and a foreign key", () => {
    const changed = Buffer.from(payload);
    changed[0] ^= 1;
    expect(verifyMinisign(changed, sig, pubA)).toBe(false);
    expect(verifyMinisign(payload, sig, pubB)).toBe(false);
    expect(verifyMinisign(payload, "not a signature", pubA)).toBe(false);
  });
});
