import { describe, expect, it } from "vitest";
import ru from "./ru.json";
import en from "./en.json";
import { resolveLang, translate } from "./index";

describe("i18n", () => {
  it("ru and en have exactly the same keys", () => {
    expect(Object.keys(ru).sort()).toEqual(Object.keys(en).sort());
  });

  it("interpolates variables", () => {
    const s = translate("ru", "errors.rate_limited", { provider: "Gemini", n: 3 });
    expect(s).toContain("Gemini");
    expect(s).toContain("3");
    expect(s).not.toContain("{");
  });

  it("picks Russian plural forms", () => {
    expect(translate("ru", "pill.pending", { n: 1 })).toBe("ещё 1 в обработке");
    expect(translate("ru", "pill.pending", { n: 2 })).toBe("ещё 2 в обработке");
    expect(translate("ru", "pill.pending", { n: 5 })).toBe("ещё 5 в обработке");
    expect(translate("en", "pill.pending", { n: 1 })).toBe("1 more processing");
  });

  it("missing keys fall back to the key itself", () => {
    expect(translate("en", "no.such.key")).toBe("no.such.key");
  });

  it("Auto follows the system language", () => {
    expect(resolveLang("Auto", "ru-RU")).toBe("ru");
    expect(resolveLang("Auto", "de-DE")).toBe("en");
    expect(resolveLang("En", "ru")).toBe("en");
    expect(resolveLang("Ru", "en-US")).toBe("ru");
  });
});
