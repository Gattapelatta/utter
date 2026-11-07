import { describe, expect, it } from "vitest";
import { isNewer, parseVersion, setVersion } from "./version";

describe("version", () => {
  it("parseVersion accepts v-prefix and rejects partial versions", () => {
    expect(parseVersion("0.2.0")).toBe("0.2.0");
    expect(parseVersion(" v0.2.1 ")).toBe("0.2.1");
    for (const bad of ["0.2", "latest", "1.0.0-beta", ""]) expect(() => parseVersion(bad)).toThrow(/0\.2\.0/);
  });

  it("isNewer compares numbers, not text", () => {
    expect(isNewer("0.10.0", "0.9.9")).toBe(true);
    expect(isNewer("0.2.0", "0.2.0")).toBe(false);
    expect(isNewer("0.1.9", "0.2.0")).toBe(false);
  });

  it("setVersion changes only the app's own version", () => {
    const files = {
      tauriConf: '{\n  "productName": "Utter",\n  "version": "0.1.0",\n  "identifier": "app.utter.desktop"\n}\n',
      cargoToml: '[package]\nname = "utter"\nversion = "0.1.0"\n\n[dependencies]\nserde = { version = "1", features = ["derive"] }\ntokio = { version = "0.1.0" }\n',
      packageJson: '{\n  "name": "utter",\n  "version": "0.1.0",\n  "dependencies": { "x": "0.1.0" }\n}\n',
    };
    const out = setVersion(files, "0.2.0");
    expect(JSON.parse(out.tauriConf).version).toBe("0.2.0");
    expect(JSON.parse(out.packageJson).version).toBe("0.2.0");
    expect(JSON.parse(out.packageJson).dependencies.x).toBe("0.1.0");
    expect(out.cargoToml).toContain('[package]\nname = "utter"\nversion = "0.2.0"');
    expect(out.cargoToml).toContain('tokio = { version = "0.1.0" }');
    expect(out.cargoToml).toContain('serde = { version = "1"');
  });
});
