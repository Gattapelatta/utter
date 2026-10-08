import type { ReactNode } from "react";

/** One setting: label, description and an optional note (e.g. an error) on the left, its control on the right. Rows are split by hairlines. */
export function Row({ label, description, note, children }: { label: string; description?: string; note?: ReactNode; children?: ReactNode }) {
  return (
    <div className="row flex items-center justify-between gap-8 border-border py-3.5">
      <div className="min-w-0">
        <div className="text-text">{label}</div>
        {description && <div className="mt-0.5 text-[13px] leading-snug text-text-secondary">{description}</div>}
        {note}
      </div>
      {children}
    </div>
  );
}
