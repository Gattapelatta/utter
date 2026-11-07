import { create } from "zustand";
import { resolveLang, systemLanguage, type Lang } from "../i18n/lang";
import { events, ipc } from "./ipc";
import { applyTheme, resolveTheme, systemIsDark, watchSystemDark, type Resolved } from "./theme";
import type { Settings } from "./types";

interface AppStore {
  settings: Settings | null;
  lang: Lang;
  theme: Resolved;
  systemDark: boolean;
  /** Loads settings and subscribes to core broadcasts. Returns an unsubscribe. */
  init(): Promise<() => void>;
  /** Saves `patch` applied on top of the latest settings (including ones changed elsewhere). */
  update(patch: Partial<Settings>): Promise<void>;
  /** Takes settings broadcast by the core (`ui://settings`). */
  applyRemote(s: Settings): void;
}

export const useApp = create<AppStore>()((set, get) => {
  const derive = (s: Settings, systemDark: boolean, animate: boolean) => {
    const theme = resolveTheme(s.theme, systemDark);
    applyTheme(theme, animate);
    set({ settings: s, theme, systemDark, lang: resolveLang(s.ui_language, systemLanguage()) });
  };

  return {
    settings: null,
    lang: resolveLang("Auto", systemLanguage()),
    theme: "dark",
    systemDark: systemIsDark(),

    async init() {
      const unlisten = events.settings((s) => get().applyRemote(s));
      const stopWatch = watchSystemDark((dark) => {
        const s = get().settings;
        if (s) derive(s, dark, true);
        else set({ systemDark: dark });
      });
      derive(await ipc.getSettings(), get().systemDark, false);
      return () => {
        stopWatch();
        unlisten.then((f) => f());
      };
    },

    async update(patch) {
      const cur = get().settings;
      if (!cur) return;
      const next = { ...cur, ...patch };
      derive(next, get().systemDark, true);
      await ipc.saveSettings(next);
    },

    applyRemote(s) {
      derive(s, get().systemDark, true);
    },
  };
});
