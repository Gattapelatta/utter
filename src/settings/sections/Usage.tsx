import { useCallback, useEffect, useState } from "react";
import { useT } from "../../i18n";
import { events, ipc } from "../../lib/ipc";
import { useApp } from "../../lib/store";
import type { FreeLimits, UsageDay } from "../../lib/types";
import { Bar } from "../ui/Bar";

type Counter = "stt_requests" | "audio_secs" | "llm_requests" | "llm_tokens";
const COUNTERS: { key: Counter; label: string }[] = [
  { key: "stt_requests", label: "provider.requests" },
  { key: "audio_secs", label: "provider.audio_minutes" },
  { key: "llm_requests", label: "provider.llm_requests" },
  { key: "llm_tokens", label: "provider.tokens" },
];
const REFRESH_MS = 5_000;

/** Bars only for counters whose free-tier limit is known. */
export function usageBars(day: UsageDay, limits: FreeLimits | null): { key: Counter; value: number; max: number }[] {
  if (!limits) return [];
  return COUNTERS.flatMap(({ key }) => {
    const max = limits[key];
    return max ? [{ key, value: day[key], max }] : [];
  });
}

/** Today's usage (§3.6), at the end of «Основное». Refreshes every 5 s and after each dictation. */
export function Usage() {
  const t = useT();
  const lang = useApp((a) => a.lang);
  const provider = useApp((a) => a.settings?.provider);
  const [day, setDay] = useState<UsageDay | null>(null);
  const [limits, setLimits] = useState<FreeLimits | null>(null);

  const refresh = useCallback(() => {
    ipc.usageToday().then(setDay).catch(() => {});
  }, []);

  useEffect(() => {
    refresh();
    const timer = setInterval(refresh, REFRESH_MS);
    const un = events.history(refresh);
    return () => {
      clearInterval(timer);
      un.then((f) => f());
    };
  }, [refresh]);

  useEffect(() => {
    if (provider) ipc.freeLimits(provider).then(setLimits).catch(() => setLimits(null));
  }, [provider]);

  const d: UsageDay = day ?? { date: "", stt_requests: 0, audio_secs: 0, llm_requests: 0, llm_tokens: 0 };
  const num = new Intl.NumberFormat(lang, { maximumFractionDigits: 1 });
  const bars = new Map(usageBars(d, limits).map((b) => [b.key, b]));
  const pct = new Intl.NumberFormat(lang, { style: "percent", maximumFractionDigits: 0 });

  return (
    <div className="mt-10">
      <h2 className="mt-0 mb-4 text-[15px] font-medium">{t("provider.today")}</h2>
      <div className="grid grid-cols-4 gap-5">
        {COUNTERS.map(({ key, label }) => {
          const value = key === "audio_secs" ? d.audio_secs / 60 : d[key];
          const bar = bars.get(key);
          return (
            <div key={key} className="min-w-0">
              <div className="text-[13px] text-text-secondary">{t(label)}</div>
              <div className="mt-1 font-mono text-[20px] leading-tight tabular-nums">{num.format(value)}</div>
              {bar && (
                <div className="mt-2">
                  <Bar value={bar.value} max={bar.max} />
                  <div className="mt-1.5 text-[12px] text-text-secondary">
                    {pct.format(Math.min(1, bar.value / bar.max))} {t("provider.free_tier")}
                  </div>
                </div>
              )}
            </div>
          );
        })}
      </div>
    </div>
  );
}
