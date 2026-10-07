import type { UiLanguage } from "../lib/types";

export type Lang = "ru" | "en";

/** "Auto" follows the system: Russian if it starts with `ru`, otherwise English. */
export function resolveLang(pref: UiLanguage, systemLang: string): Lang {
  if (pref === "Ru") return "ru";
  if (pref === "En") return "en";
  return systemLang.toLowerCase().startsWith("ru") ? "ru" : "en";
}

export function systemLanguage(): string {
  return typeof navigator === "undefined" ? "en" : navigator.language;
}
