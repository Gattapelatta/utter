import { useCallback, useEffect, useRef, useState } from "react";
import { ArrowUpRight, CircleNotch, WarningCircle } from "@phosphor-icons/react";
import { useT } from "../../../i18n";
import { ipc } from "../../../lib/ipc";
import type { ProviderKind } from "../../../lib/types";
import { Button } from "../../ui/Button";
import { TextField } from "../../ui/TextField";
import { Done } from "./Status";

const POLL_MS = 1_500;
const LINKS = { Gemini: "gemini_key", Groq: "groq_key", OpenAi: "openai_key" } as const;

/**
 * Three numbered actions; the key is picked up from the clipboard when the user comes back
 * (focus, and every 1.5 s while the step is open), saved and verified by itself (§4.4).
 */
export function ProviderKey({
  provider,
  ok,
  onVerified,
}: {
  provider: ProviderKind;
  ok: boolean;
  onVerified: (checked: boolean) => void;
}) {
  const t = useT();
  const [checking, setChecking] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [manual, setManual] = useState(false);
  const [draft, setDraft] = useState("");
  const tried = useRef<string | null>(null);
  const live = useRef(true);
  useEffect(
    () => () => {
      live.current = false;
    },
    [],
  );

  const tryKey = useCallback(
    async (key: string) => {
      if (tried.current === key) return;
      tried.current = key;
      setError(null);
      setChecking(true);
      try {
        await ipc.setApiKey(provider, key);
        await ipc.verifyKey(provider);
        if (live.current) onVerified(true);
      } catch (e) {
        if (live.current) setError(String(e));
      } finally {
        if (live.current) setChecking(false);
      }
    },
    [provider, onVerified],
  );

  useEffect(() => {
    if (ok) return;
    const look = () => {
      ipc
        .clipboardKey(provider)
        .then((k) => {
          if (k) tryKey(k);
        })
        .catch(() => {});
    };
    look();
    window.addEventListener("focus", look);
    const id = setInterval(look, POLL_MS);
    return () => {
      window.removeEventListener("focus", look);
      clearInterval(id);
    };
  }, [ok, provider, tryKey]);

  if (ok) return <Done label={t("onboarding.key_ok")} />;

  const n = (i: number) => (
    <span className="flex size-[22px] shrink-0 items-center justify-center rounded-full bg-surface-raised text-[12px] text-text">{i}</span>
  );
  return (
    <div className="inline-flex flex-col items-start gap-1 text-left">
      <div className="flex items-center gap-3 py-2">
        {n(1)}
        <Button variant="primary" onClick={() => ipc.openLink(LINKS[provider]).catch(() => {})}>
          {t("onboarding.key_open")}
          <ArrowUpRight size={14} aria-hidden />
        </Button>
      </div>
      <div className="flex items-center gap-3 py-2 text-text">
        {n(2)}
        {t("onboarding.key_press")}
        <span className="inline-flex h-6 items-center rounded-md bg-[#1a73e8] px-2.5 text-[12px] text-white">Create API key</span>
        {t("onboarding.key_copy")}
      </div>
      <div className="flex items-center gap-3 py-2 text-text">
        {n(3)}
        {t("onboarding.key_return")}
      </div>
      <div role="status" className="mt-2 flex min-h-6 items-center gap-2 text-[13px]">
        {error ? (
          <span className="flex items-center gap-1.5 text-text">
            <WarningCircle data-accent size={16} className="text-danger" aria-hidden />
            {t(`errors.${error}`, { provider: "Gemini", n: 0 })}
            {error === "network" && (
              <Button variant="ghost" onClick={() => onVerified(false)}>
                {t("onboarding.continue_unverified")}
              </Button>
            )}
          </span>
        ) : (
          <span className="flex items-center gap-2 text-text-secondary">
            <CircleNotch size={14} className="spin" aria-hidden />
            {checking ? t("onboarding.key_checking") : t("onboarding.key_waiting")}
          </span>
        )}
      </div>
      {manual ? (
        <form
          className="mt-2 flex w-[340px] items-center gap-2"
          onSubmit={(e) => {
            e.preventDefault();
            if (draft.trim()) {
              tried.current = null;
              tryKey(draft.trim());
            }
          }}
        >
          <TextField autoFocus spellCheck={false} value={draft} onChange={(e) => setDraft(e.target.value)} placeholder={t("provider.key_placeholder")} className="font-mono text-[13px]" />
          <Button type="submit">{t("common.save")}</Button>
        </form>
      ) : (
        <button type="button" onClick={() => setManual(true)} className="link mt-1 text-[13px] text-text-secondary">
          {t("onboarding.key_manual")}
        </button>
      )}
    </div>
  );
}
