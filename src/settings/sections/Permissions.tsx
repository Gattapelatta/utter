import { useCallback, useEffect, useState } from "react";
import { useT } from "../../i18n";
import { ipc } from "../../lib/ipc";
import type { PermissionsStatus } from "../../lib/types";
import { Accessibility } from "../onboarding/steps/Accessibility";
import { Microphone } from "../onboarding/steps/Microphone";
import { Button } from "../ui/Button";
import { Row } from "../ui/Row";
import { Section } from "./Section";

const POLL_MS = 1_000;

/** Opened from an error pill: both permissions in one place, polled once a second. */
export function Permissions({ onDone }: { onDone: () => void }) {
  const t = useT();
  const [perms, setPerms] = useState<PermissionsStatus | null>(null);
  const refresh = useCallback(() => {
    ipc.permissionsStatus().then(setPerms).catch(() => {});
  }, []);
  useEffect(() => {
    refresh();
    const id = setInterval(refresh, POLL_MS);
    return () => clearInterval(id);
  }, [refresh]);

  return (
    <Section route="permissions" subtitle={t("permissions.subtitle")}>
      <Row label={t("onboarding.mic_title")} description={t("onboarding.mic_body")}>
        <Microphone perms={perms} refresh={refresh} />
      </Row>
      <Row label={t("onboarding.ax_title")} description={t("onboarding.ax_body")}>
        <Accessibility perms={perms} />
      </Row>
      <div className="mt-8 flex justify-end">
        <Button variant="primary" onClick={onDone}>
          {t("permissions.done")}
        </Button>
      </div>
    </Section>
  );
}
