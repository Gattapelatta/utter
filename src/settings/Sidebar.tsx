import { ClockCounterClockwise, Info, Key, Keyboard, SlidersHorizontal, Translate, type Icon } from "@phosphor-icons/react";
import { useT } from "../i18n";
import { Logo } from "./Logo";
import { NAV_ROUTES, type Route } from "./routes";

const ICONS: Partial<Record<Route, Icon>> = {
  general: SlidersHorizontal,
  hotkeys: Keyboard,
  translate: Translate,
  provider: Key,
  history: ClockCounterClockwise,
  about: Info,
};

/**
 * The logo sits under the traffic lights; the section list is centred on the window's
 * height (equal space above and below it, §4.3), whatever the window size.
 */
export function Sidebar({ route, onGo }: { route: Route; onGo: (r: Route) => void }) {
  const t = useT();
  return (
    <nav className="relative h-full w-[232px] shrink-0">
      <div data-tauri-drag-region className="h-[52px]" />
      <Logo />
      <ul data-nav className="absolute inset-x-0 top-1/2 m-0 -translate-y-1/2 list-none px-4 py-0">
        {NAV_ROUTES.map((r) => {
          const Icon = ICONS[r]!;
          const on = r === route;
          return (
            <li key={r}>
              <button
                type="button"
                aria-current={on ? "page" : undefined}
                onClick={() => onGo(r)}
                className={`nav-item flex h-9 w-full items-center gap-2.5 rounded-lg px-3 text-left text-[14px] ${on ? "text-text" : "text-text-secondary"}`}
              >
                <Icon size={18} weight={on ? "regular" : "light"} aria-hidden />
                {t(`nav.${r}`)}
              </button>
            </li>
          );
        })}
      </ul>
    </nav>
  );
}
