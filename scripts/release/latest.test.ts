import { describe, expect, it } from "vitest";
import { assetUrl, buildLatestJson } from "./latest";

describe("latest.json", () => {
  it("latest.json lists both Mac platforms with the same universal archive", () => {
    const j = buildLatestJson({ version: "0.2.0", notes: "n", pubDate: new Date("2026-11-08T10:00:00Z"), signature: "SIG", url: assetUrl("0.2.0", "Utter.app.tar.gz") });
    expect(Object.keys(j.platforms).sort()).toEqual(["darwin-aarch64", "darwin-x86_64"]);
    expect(j.platforms["darwin-x86_64"]).toEqual(j.platforms["darwin-aarch64"]);
    expect(j.platforms["darwin-aarch64"].url).toBe("https://github.com/Gattapelatta/utter/releases/download/v0.2.0/Utter.app.tar.gz");
    expect(j.platforms["darwin-aarch64"].signature).toBe("SIG");
    expect(j.pub_date).toBe("2026-11-08T10:00:00.000Z");
    expect(j.version).toBe("0.2.0");
    expect(j.notes).toBe("n");
  });
});
