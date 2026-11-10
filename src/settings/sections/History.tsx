import { useCallback, useEffect, useRef, useState } from "react";
import { Check, Copy, WarningCircle } from "@phosphor-icons/react";
import { useT } from "../../i18n";
import { events, ipc } from "../../lib/ipc";
import { useApp } from "../../lib/store";
import type { HistoryEntry } from "../../lib/types";
import { Button } from "../ui/Button";
import { Row } from "../ui/Row";
import { Switch } from "../ui/Switch";
import { Section } from "./Section";

const COPIED_MS = 1_500;

/** Known error keys get their translation; anything else is shown as recorded. */
function errorText(message: string, t: (k: string, v?: Record<string, string | number>) => string) {
  const key = `errors.${message}`;
  const text = t(key, { provider: "", n: 0 });
  return text === key ? message : text;
}

function sameDay(a: Date, b: Date) {
  return a.getFullYear() === b.getFullYear() && a.getMonth() === b.getMonth() && a.getDate() === b.getDate();
}

function Entry({ e }: { e: HistoryEntry }) {
  const t = useT();
  const lang = useApp((a) => a.lang);
  const [open, setOpen] = useState(false);
  const [copied, setCopied] = useState(false);
  const timer = useRef<ReturnType<typeof setTimeout>>(undefined);
  useEffect(() => () => clearTimeout(timer.current), []);

  const at = new Date(e.created_at_ms);
  const when = new Intl.DateTimeFormat(
    lang,
    sameDay(at, new Date()) ? { hour: "2-digit", minute: "2-digit" } : { day: "numeric", month: "short", hour: "2-digit", minute: "2-digit" },
  ).format(at);
  const failed = e.status.kind === "failed";
  const note =
    e.status.kind === "degraded"
      ? t("history.degraded")
      : e.status.kind === "nothing_heard"
        ? t("history.nothing_heard")
        : e.status.kind === "failed"
          ? t("history.failed", { message: errorText(e.status.message, t) })
          : null;

  const copy = async () => {
    await ipc.copyText(e.text).catch(() => {});
    setCopied(true);
    clearTimeout(timer.current);
    timer.current = setTimeout(() => setCopied(false), COPIED_MS);
  };

  return (
    <li data-entry className="row flex items-start gap-4 border-border py-3.5">
      <div className="min-w-0 flex-1">
        <div className="flex flex-wrap items-center gap-x-2 text-[12px] text-text-secondary">
          <span className="font-mono tabular-nums">{when}</span>
          <span>{e.mode === "Translate" ? t("history.translate") : e.mode === "Task" ? t("history.task") : t("history.dictation")}</span>
          {note && (
            <span className="flex items-center gap-1">
              <span aria-hidden>·</span>
              {failed && <WarningCircle data-accent size={13} className="text-danger" aria-hidden />}
              {note}
            </span>
          )}
        </div>
        {e.text && (
          <p
            data-text
            data-clamped={open ? undefined : ""}
            onClick={() => setOpen((v) => !v)}
            className={`mt-1 mb-0 cursor-default leading-normal whitespace-pre-wrap text-text select-text ${open ? "" : "line-clamp-3"}`}
          >
            {e.text}
          </p>
        )}
        {open && e.raw && e.raw !== e.text && (
          <div className="mt-2.5">
            <div className="text-[12px] text-text-secondary">{t("history.raw")}</div>
            <p className="mt-0.5 mb-0 leading-normal whitespace-pre-wrap text-text-secondary select-text">{e.raw}</p>
          </div>
        )}
      </div>
      <div className="flex shrink-0 items-center gap-1 pt-4">
        {failed && e.audio_file ? (
          <Button onClick={() => ipc.historyRetry(e.id).catch(() => {})}>{t("common.retry")}</Button>
        ) : (
          e.text && (
            <Button variant="ghost" aria-label={copied ? t("common.copied") : t("common.copy")} onClick={copy} className="px-2">
              {copied ? <Check data-accent size={16} className="text-success" aria-hidden /> : <Copy size={16} aria-hidden />}
            </Button>
          )
        )}
      </div>
    </li>
  );
}

export function History() {
  const t = useT();
  const enabled = useApp((a) => a.settings?.history_enabled ?? true);
  const update = useApp((a) => a.update);
  const [entries, setEntries] = useState<HistoryEntry[] | null>(null);
  const [confirming, setConfirming] = useState(false);

  const load = useCallback(() => {
    ipc.historyList().then(setEntries).catch(() => setEntries([]));
  }, []);

  useEffect(() => {
    load();
    const un = events.history(load);
    return () => {
      un.then((f) => f());
    };
  }, [load]);

  const toggle = (on: boolean) => {
    if (on) update({ history_enabled: true });
    else setConfirming(true);
  };

  return (
    <Section route="history" subtitle={t("history.subtitle")}>
      <Row
        label={t("history.save")}
        note={
          confirming && (
            <div className="mt-2 flex items-center gap-2">
              <span className="text-[13px] text-text">{t("history.confirm_clear")}</span>
              <Button
                variant="primary"
                onClick={async () => {
                  setConfirming(false);
                  await update({ history_enabled: false });
                  load();
                }}
              >
                {t("history.delete")}
              </Button>
              <Button variant="ghost" onClick={() => setConfirming(false)}>
                {t("common.cancel")}
              </Button>
            </div>
          )
        }
      >
        <Switch label={t("history.save")} checked={enabled && !confirming} onChange={toggle} />
      </Row>
      {entries && entries.length === 0 && <p className="mt-10 text-center text-text-secondary">{t("history.empty")}</p>}
      {entries && entries.length > 0 && (
        <ul className="m-0 list-none border-t border-border p-0">
          {entries.map((e) => (
            <Entry key={e.id} e={e} />
          ))}
        </ul>
      )}
    </Section>
  );
}
