import type { ReactNode } from "react";
import { useT } from "../../i18n";
import type { Route } from "../routes";

const TITLES: Record<Route, string> = {
  onboarding: "onboarding.welcome_title",
  permissions: "permissions.title",
  general: "nav.general",
  hotkeys: "nav.hotkeys",
  translate: "nav.translate",
  provider: "nav.provider",
  history: "nav.history",
  about: "nav.about",
};

/** A section page: the large title (§4.3) and its content. */
export function Section({ route, subtitle, children }: { route: Route; subtitle?: string; children?: ReactNode }) {
  const t = useT();
  return (
    <section className="mx-auto max-w-[640px] px-10 pt-14 pb-10">
      <h1 className="mt-0 mb-6 text-[28px] leading-[1.1] font-[450] tracking-[-0.02em]">{t(TITLES[route])}</h1>
      {subtitle && <p className="-mt-3 mb-6 text-text-secondary">{subtitle}</p>}
      {children}
    </section>
  );
}
