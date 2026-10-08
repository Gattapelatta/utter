/** Speech languages offered in Settings (codes match `providers/gemini.rs`). `null` = detect automatically. */
export interface Language {
  code: string | null;
  name: string;
}

const MAIN: Language[] = [
  { code: null, name: "" }, // "Auto", translated by the UI
  { code: "ru", name: "Русский" },
  { code: "en", name: "English" },
  { code: "uk", name: "Українська" },
  { code: "de", name: "Deutsch" },
  { code: "es", name: "Español" },
  { code: "fr", name: "Français" },
];

const OTHER: Language[] = [
  { code: "it", name: "Italiano" },
  { code: "pt", name: "Português" },
  { code: "pl", name: "Polski" },
  { code: "tr", name: "Türkçe" },
  { code: "ja", name: "日本語" },
  { code: "zh", name: "中文" },
  { code: "ko", name: "한국어" },
  { code: "nl", name: "Nederlands" },
  { code: "sv", name: "Svenska" },
  { code: "fi", name: "Suomi" },
  { code: "da", name: "Dansk" },
  { code: "no", name: "Norsk" },
  { code: "cs", name: "Čeština" },
  { code: "sk", name: "Slovenčina" },
  { code: "ro", name: "Română" },
  { code: "hu", name: "Magyar" },
  { code: "el", name: "Ελληνικά" },
  { code: "he", name: "עברית" },
  { code: "ar", name: "العربية" },
  { code: "hi", name: "हिन्दी" },
  { code: "id", name: "Bahasa Indonesia" },
  { code: "vi", name: "Tiếng Việt" },
  { code: "th", name: "ไทย" },
  { code: "bg", name: "Български" },
  { code: "hr", name: "Hrvatski" },
  { code: "sr", name: "Српски" },
  { code: "lt", name: "Lietuvių" },
  { code: "lv", name: "Latviešu" },
  { code: "et", name: "Eesti" },
  { code: "kk", name: "Қазақ" },
  { code: "be", name: "Беларуская" },
  { code: "ka", name: "ქართული" },
  { code: "hy", name: "Հայերեն" },
  { code: "az", name: "Azərbaycan" },
  { code: "uz", name: "Oʻzbek" },
  { code: "fa", name: "فارسی" },
  { code: "ms", name: "Bahasa Melayu" },
].sort((a, b) => a.name.localeCompare(b.name));

export const MAIN_LANGUAGE_COUNT = MAIN.length;
export const SPEECH_LANGUAGES: Language[] = [...MAIN, ...OTHER];
