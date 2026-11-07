export function Switch({ checked, onChange, label }: { checked: boolean; onChange: (v: boolean) => void; label?: string }) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={label}
      data-accent
      onClick={() => onChange(!checked)}
      className={`switch relative h-5 w-[34px] shrink-0 rounded-full ${checked ? "bg-success" : "bg-surface-raised"}`}
    >
      <span
        className={`switch-knob absolute top-0.5 left-0.5 size-4 rounded-full ${checked ? "bg-white" : "bg-text-secondary"}`}
        style={{ transform: checked ? "translateX(14px)" : "translateX(0)" }}
      />
    </button>
  );
}
