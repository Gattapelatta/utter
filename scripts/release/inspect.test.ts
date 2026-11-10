import { describe, expect, it } from "vitest";
import { helperProblem, minimumSystemProblem, parseCodesign, parseLipo, parseLeaf } from "./inspect";

const signed = `Executable=/tmp/Utter.app/Contents/MacOS/utter
Identifier=app.utter.desktop
Format=app bundle with Mach-O universal (x86_64 arm64)
CodeDirectory v=20500 size=1234 flags=0x0(none) hashes=30+7 location=embedded
Signature size=1660
Authority=Utter Dev
Signed Time=7 Nov 2026 at 15:00:00
Info.plist entries=20
TeamIdentifier=not set
`;
const adhoc = `Executable=/tmp/Utter.app/Contents/MacOS/utter
Identifier=utter-55554944
Format=app bundle with Mach-O thin (arm64)
Signature=adhoc
TeamIdentifier=not set
`;

describe("inspect", () => {
  it("parseCodesign reads the signer and the identifier", () => {
    expect(parseCodesign(signed)).toEqual({ authority: "Utter Dev", identifier: "app.utter.desktop" });
    expect(parseCodesign(adhoc)).toEqual({ authority: null, identifier: "utter-55554944" });
  });

  it("parseLipo lists architectures", () => {
    expect(parseLipo("x86_64 arm64\n")).toEqual(["x86_64", "arm64"]);
  });

  // Final review I1: a re-created "Utter Dev" certificate has the same name but another leaf;
  // every user's Accessibility permission would silently stop matching.
  it("parseLeaf reads the certificate the requirement is pinned to", () => {
    const out = `Executable=/x/Utter.app/Contents/MacOS/utter
designated => identifier "app.utter.desktop" and certificate leaf = H"543970408eaaf9e3c062c11084bc7b9accb9e61b"
`;
    expect(parseLeaf(out)).toBe("543970408eaaf9e3c062c11084bc7b9accb9e61b");
    expect(parseLeaf('designated => cdhash H"abc"')).toBeNull();
  });

  // The README promises macOS 13+, but the built app said 10.13 (Tauri's default): an older Mac
  // would install it and fail instead of being told up front.
  it("minimumSystemProblem wants the app to require macOS 13", () => {
    expect(minimumSystemProblem("13.0")).toBeNull();
    expect(minimumSystemProblem("10.13")).toMatch(/10\.13/);
    expect(minimumSystemProblem(null)).toMatch(/не указана/);
  });

  // Agents run utter-agent from inside the app: without it, or without one architecture, the Drop
  // silently hears nothing on half of the Macs.
  it("helperProblem wants utter-agent for both architectures", () => {
    expect(helperProblem(["x86_64", "arm64"])).toBeNull();
    expect(helperProblem(null)).toMatch(/utter-agent/);
    expect(helperProblem(["arm64"])).toMatch(/x86_64/);
  });
});
