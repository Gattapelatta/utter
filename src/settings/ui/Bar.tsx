/** A thin usage bar; turns amber past 80 %. */
export function Bar({ value, max }: { value: number; max: number }) {
  const p = max > 0 ? Math.max(0, Math.min(1, value / max)) : 0;
  return (
    <div role="progressbar" aria-valuenow={value} aria-valuemin={0} aria-valuemax={max} className="h-1.5 w-full overflow-hidden rounded-full bg-surface-raised">
      <div
        data-accent={p > 0.8 ? "" : undefined}
        className={`bar-fill h-full origin-left rounded-full ${p > 0.8 ? "bg-warning" : "bg-text"}`}
        style={{ transform: `scaleX(${p})` }}
      />
    </div>
  );
}
