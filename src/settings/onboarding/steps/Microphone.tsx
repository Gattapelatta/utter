import { useT } from "../../../i18n";
import { ipc } from "../../../lib/ipc";
import type { PermissionsStatus } from "../../../lib/types";
import { Button } from "../../ui/Button";
import { Done } from "./Status";

export function Microphone({ perms, refresh }: { perms: PermissionsStatus | null; refresh: () => void }) {
  const t = useT();
  if (perms?.microphone === "Granted") return <Done label={t("common.granted")} />;
  if (perms?.microphone === "Denied")
    return (
      <div className="flex flex-col items-center gap-3">
        <p className="m-0 text-text">{t("onboarding.mic_denied")}</p>
        <Button variant="primary" className="h-10 px-5 text-[14px]" onClick={() => ipc.openMicrophoneSettings().catch(() => {})}>
          {t("common.open_settings")}
        </Button>
      </div>
    );
  return (
    <Button variant="primary" className="h-10 px-5 text-[14px]" onClick={() => ipc.requestMicrophone().finally(refresh)}>
      {t("onboarding.mic_button")}
    </Button>
  );
}
