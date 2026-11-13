/** `color` / `knob`: the agent's brand colours when on (the user 2026-11-13); the default is green. */
export function Switch({
  checked,
  onChange,
  label,
  disabled = false,
  color,
  knob,
}: {
  checked: boolean;
  onChange: (v: boolean) => void;
  label?: string;
  disabled?: boolean;
  color?: string;
  knob?: string;
}) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={label}
      data-accent
      disabled={disabled}
      onClick={() => onChange(!checked)}
      className={`switch relative h-5 w-[34px] shrink-0 rounded-full ${checked ? "bg-success" : "bg-surface-raised"} ${disabled ? "opacity-40" : ""}`}
      style={checked && color ? { background: color } : undefined}
    >
      <span
        className={`switch-knob absolute top-0.5 left-0.5 size-4 rounded-full ${checked ? "bg-white" : "bg-text-secondary"}`}
        style={{ transform: checked ? "translateX(14px)" : "translateX(0)", ...(checked && knob ? { background: knob } : {}) }}
      />
    </button>
  );
}
