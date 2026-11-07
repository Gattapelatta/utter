import { useApp } from "../lib/store";
import en from "./en.json";
import type { Lang } from "./lang";
import ru from "./ru.json";

export { resolveLang, systemLanguage, type Lang } from "./lang";
export type Vars = Record<string, string | number>;

const dicts: Record<Lang, Record<string, string>> = { ru, en };

/** Looks up `key`; with `vars.n` prefers the plural form `key_one/_few/_many/_other`. */
export function translate(lang: Lang, key: string, vars?: Vars): string {
  const dict = dicts[lang];
  let text: string | undefined;
  if (vars && typeof vars.n === "number") {
    const form = new Intl.PluralRules(lang).select(vars.n);
    text = dict[`${key}_${form}`] ?? dict[`${key}_other`];
  }
  text ??= dict[key];
  if (text === undefined) return key;
  return vars ? text.replace(/\{(\w+)\}/g, (m, name) => (name in vars ? String(vars[name]) : m)) : text;
}

export function useT(): (key: string, vars?: Vars) => string {
  const lang = useApp((s) => s.lang);
  return (key, vars) => translate(lang, key, vars);
}
