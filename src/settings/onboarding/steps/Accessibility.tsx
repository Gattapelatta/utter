import { useT } from "../../../i18n";
import { ipc } from "../../../lib/ipc";
import type { PermissionsStatus } from "../../../lib/types";
import { Button } from "../../ui/Button";
import { Done } from "./Status";

/** Shows, instead of explaining, which switch to turn on in System Settings. */
export function Accessibility({ perms }: { perms: PermissionsStatus | null }) {
  const t = useT();
  if (perms?.accessibility) return <Done label={t("common.granted")} />;
  return (
    <div className="flex flex-col items-center gap-4">
      <div aria-hidden className="inline-flex items-center gap-2.5 rounded-[10px] border border-border bg-surface px-3 py-2">
        <svg viewBox="13 14 74 76" width="18" height="18">
          <path d="M50 14 C74 14 87 36 87 58 C87 77 71 90 50 90 C29 90 13 77 13 58 C13 36 26 14 50 14Z" style={{ fill: "var(--drop-body, #fff)" }} />
        </svg>
        <span className="text-text">Utter</span>
        <span data-accent className="relative ml-6 inline-block h-[15px] w-[26px] rounded-full bg-success">
          <span className="absolute top-0.5 right-0.5 size-[11px] rounded-full bg-white" />
        </span>
      </div>
      <Button variant="primary" className="h-10 px-5 text-[14px]" onClick={() => ipc.openAccessibilitySettings().catch(() => {})}>
        {t("onboarding.ax_button")}
      </Button>
    </div>
  );
}
