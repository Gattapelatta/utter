import { describe, expect, it } from "vitest";
import { translate } from "../i18n";
import { keyLabel, typesText } from "./keys";

const ru = (k: string, v?: Record<string, string | number>) => translate("ru", k, v);

describe("key names (any key can be a hotkey, the user 2026-11-10)", () => {
  it("names the left modifiers", () => {
    expect(keyLabel("LeftCtrl", ru)).toBe("Левый ⌃");
    expect(keyLabel("LeftAlt", ru)).toBe("Левый ⌥");
    expect(keyLabel("LeftMeta", ru)).toBe("Левый ⌘");
    expect(keyLabel("LeftShift", ru)).toBe("Левый ⇧");
  });

  it("names ordinary keys by what is printed on them (Mac key codes)", () => {
    expect(keyLabel({ Raw: 0 }, ru)).toBe("A");
    expect(keyLabel({ Raw: 18 }, ru)).toBe("1");
    expect(keyLabel({ Raw: 49 }, ru)).toBe("Пробел");
    expect(keyLabel({ Raw: 126 }, ru)).toBe("↑");
    expect(keyLabel({ Raw: 9999 }, ru)).toBe("Клавиша 9999");
  });

  it("knows which keys type text", () => {
    expect(typesText({ Raw: 0 })).toBe(true);
    expect(typesText("LeftCtrl")).toBe(false);
    expect(typesText({ F: 5 })).toBe(false);
  });
});
