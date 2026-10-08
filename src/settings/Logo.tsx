/**
 * The living logo (§4.3): this only marks the Drop's place — the Drop itself is the
 * window's one `FloatingDrop`, so it can fly between the logo and other pages.
 */
export function Logo() {
  return (
    <div data-logo className="flex items-center gap-3 px-6">
      <div data-drop-slot="logo" className="shrink-0" style={{ width: 60, height: 63 }} />
      <span data-intro-item className="text-[22px] leading-none font-medium tracking-[-0.015em]">
        Utter
      </span>
    </div>
  );
}
