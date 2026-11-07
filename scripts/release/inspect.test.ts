import { describe, expect, it } from "vitest";
import { parseCodesign, parseLipo } from "./inspect";

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
});
