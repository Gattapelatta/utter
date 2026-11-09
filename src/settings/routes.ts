export const ROUTES = ["onboarding", "permissions", "general", "hotkeys", "translate", "provider", "agents", "history", "about"] as const;
export type Route = (typeof ROUTES)[number];

/** Sections listed in the sidebar, in order (spec §4.3). `permissions` opens only from errors. */
export const NAV_ROUTES: Route[] = ["general", "hotkeys", "translate", "provider", "agents", "history", "about"];

export function parseRoute(hash: string): Route {
  const r = hash.replace(/^#\/?/, "");
  return (ROUTES as readonly string[]).includes(r) ? (r as Route) : "general";
}
