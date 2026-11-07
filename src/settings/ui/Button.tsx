import type { ButtonHTMLAttributes } from "react";

type Variant = "primary" | "secondary" | "ghost";

const look: Record<Variant, string> = {
  primary: "bg-text text-bg hover:opacity-90",
  secondary: "bg-surface-raised text-text hover:bg-border-strong",
  ghost: "text-text-secondary hover:text-text",
};

/** Pill button: presses in on :active (0.97, 120 ms), colours ease over 150 ms (§5.4). */
export function Button({ variant = "secondary", className = "", ...rest }: ButtonHTMLAttributes<HTMLButtonElement> & { variant?: Variant }) {
  return (
    <button
      type="button"
      data-variant={variant}
      className={`btn inline-flex h-8 items-center gap-1.5 rounded-full px-4 text-[13px] font-medium select-none disabled:opacity-40 ${look[variant]} ${className}`}
      {...rest}
    />
  );
}
