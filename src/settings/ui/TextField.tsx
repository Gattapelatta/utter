import type { InputHTMLAttributes } from "react";

export function TextField({ className = "", ...rest }: InputHTMLAttributes<HTMLInputElement>) {
  return (
    <input
      className={`h-9 w-full rounded-lg border border-border bg-surface px-3 text-text outline-none placeholder:text-text-secondary focus:border-border-strong ${className}`}
      {...rest}
    />
  );
}
