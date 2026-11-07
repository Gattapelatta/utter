import { useEffect, useState } from "react";
import { useT } from "../../../i18n";
import { events, ipc } from "../../../lib/ipc";
import type { TrialOutcome } from "../../../lib/types";

/** While this step is shown, dictations land here instead of being pasted (§4.4). */
export function Trial({ onResult }: { onResult: () => void }) {
  const t = useT();
  const [outcome, setOutcome] = useState<TrialOutcome | null>(null);

  useEffect(() => {
    ipc.setTrial(true).catch(() => {});
    const un = events.trial((o) => {
      setOutcome(o);
      if (o.kind === "text") onResult();
    });
    return () => {
      ipc.setTrial(false).catch(() => {});
      un.then((f) => f());
    };
  }, [onResult]);

  if (!outcome) return <p className="m-0 text-text-secondary">{t("onboarding.trial_waiting")}</p>;
  if (outcome.kind === "error") return <p className="m-0 text-text">{t(`errors.${outcome.message_key}`, { provider: "", n: 0 })}</p>;
  return <div className="mx-auto max-w-[460px] rounded-xl border border-border bg-surface px-4 py-3.5 text-left text-[17px] leading-normal text-text">{outcome.text}</div>;
}
