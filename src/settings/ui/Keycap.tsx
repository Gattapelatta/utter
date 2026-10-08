/** A key label drawn as a physical key; sinks by 2 px while the real key is held (80 ms, §5.4). */
export function Keycap({ label, pressed = false }: { label: string; pressed?: boolean }) {
  return (
    <kbd
      data-pressed={pressed ? "" : undefined}
      className="keycap inline-flex h-8 min-w-8 items-center justify-center rounded-lg border border-border-strong bg-surface-raised px-2.5 font-mono text-[13px] text-text"
    >
      {label}
    </kbd>
  );
}
