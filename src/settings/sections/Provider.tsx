import { useCallback, useEffect, useState } from "react";
import { ArrowUpRight, CheckCircle, WarningCircle } from "@phosphor-icons/react";
import { useT } from "../../i18n";
import { ipc } from "../../lib/ipc";
import { useApp } from "../../lib/store";
import type { ProviderKind } from "../../lib/types";
import { Button } from "../ui/Button";
import { TextField } from "../ui/TextField";
import { Section } from "./Section";

const PROVIDERS: { kind: ProviderKind; name: string; caption: string; link: "gemini_key" | "groq_key" | "openai_key"; free: boolean }[] = [
  { kind: "Gemini", name: "Google Gemini", caption: "provider.gemini_caption", link: "gemini_key", free: true },
  { kind: "Groq", name: "Groq", caption: "provider.groq_caption", link: "groq_key", free: true },
  { kind: "OpenAi", name: "OpenAI", caption: "provider.openai_caption", link: "openai_key", free: false },
];

type Status = { kind: "verifying" } | { kind: "ok" } | { kind: "error"; key: string } | null;

export function Provider() {
  const t = useT();
  const provider = useApp((a) => a.settings?.provider);
  const update = useApp((a) => a.update);
  const [mask, setMask] = useState<string | null>(null);
  const [draft, setDraft] = useState("");
  const [status, setStatus] = useState<Status>(null);

  const loadMask = useCallback((p: ProviderKind) => {
    ipc.apiKeyMask(p).then(setMask).catch(() => setMask(null));
  }, []);

  useEffect(() => {
    if (!provider) return;
    setStatus(null);
    setDraft("");
    loadMask(provider);
  }, [provider, loadMask]);

  if (!provider) return null;
  const current = PROVIDERS.find((p) => p.kind === provider)!;

  const save = async () => {
    const key = draft.trim();
    if (!key) return;
    await ipc.setApiKey(provider, key);
    setDraft("");
    setStatus(null);
    loadMask(provider);
  };

  const verify = async () => {
    setStatus({ kind: "verifying" });
    try {
      await ipc.verifyKey(provider);
      setStatus({ kind: "ok" });
    } catch (e) {
      setStatus({ kind: "error", key: String(e) });
    }
  };

  return (
    <Section route="provider" subtitle={t("provider.subtitle")}>
      <div role="radiogroup" aria-label={t("provider.title")}>
        {PROVIDERS.map((p) => {
          const on = p.kind === provider;
          return (
            <button
              key={p.kind}
              type="button"
              role="radio"
              aria-checked={on}
              onClick={() => !on && update({ provider: p.kind })}
              className="row flex w-full items-center gap-3 border-border py-3 text-left"
            >
              <span className={`radio flex size-4 shrink-0 items-center justify-center rounded-full border-[1.5px] ${on ? "border-text" : "border-text-secondary"}`}>
                <span className="radio-dot size-2 rounded-full bg-text" style={{ opacity: on ? 1 : 0, transform: on ? "scale(1)" : "scale(0.5)" }} />
              </span>
              <span className="min-w-0">
                <span className="text-text">
                  {p.name}
                  {p.kind === "Gemini" && (
                    <span className="ml-2 rounded-full bg-surface-raised px-2 py-px align-[1px] text-[11px] text-text-secondary">{t("provider.recommended")}</span>
                  )}
                </span>
                <span className="mt-0.5 block text-[13px] text-text-secondary">{t(p.caption)}</span>
              </span>
            </button>
          );
        })}
      </div>

      <div className="mt-2 border-t border-border pt-5">
        <label htmlFor="api-key" className="mb-2 block text-text">
          {t("provider.key")}
        </label>
        <form
          className="flex items-center gap-2"
          onSubmit={(e) => {
            e.preventDefault();
            save();
          }}
        >
          <TextField
            id="api-key"
            type="password"
            autoComplete="off"
            spellCheck={false}
            value={draft}
            onChange={(e) => setDraft(e.target.value)}
            placeholder={mask ?? t("provider.key_placeholder")}
            className="font-mono text-[13px]"
          />
          <Button type="submit" disabled={!draft.trim()}>
            {t("common.save")}
          </Button>
          <Button onClick={verify} disabled={status?.kind === "verifying"}>
            {status?.kind === "verifying" ? t("provider.verifying") : t("provider.verify")}
          </Button>
        </form>
        <div className="mt-2.5 flex min-h-6 items-center justify-between gap-4 text-[13px]">
          <span role="status" className="flex items-center gap-1.5 text-text">
            {status?.kind === "ok" && (
              <>
                <CheckCircle data-accent size={16} weight="fill" className="text-success" aria-hidden />
                {t("provider.verified")}
              </>
            )}
            {status?.kind === "error" && (
              <>
                <WarningCircle data-accent size={16} className="text-danger" aria-hidden />
                {t(`errors.${status.key}`, { provider: current.name, n: 0 })}
              </>
            )}
            {!status && !mask && <span className="text-text-secondary">{t("provider.key_missing")}</span>}
          </span>
          <button type="button" onClick={() => ipc.openLink(current.link).catch(() => {})} className="link flex items-center gap-1 text-text-secondary">
            {current.free ? t("provider.get_free_key") : t("provider.get_key")}
            <ArrowUpRight size={13} aria-hidden />
          </button>
        </div>
      </div>
    </Section>
  );
}
