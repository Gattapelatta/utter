import { useEffect, useRef, useState } from "react";
import { ArrowUpRight, Check } from "@phosphor-icons/react";
import { useT } from "../../i18n";
import { events, ipc } from "../../lib/ipc";
import type { UpdateState } from "../../lib/types";
import { useDropStage } from "../drop/dropStage";
import { Button } from "../ui/Button";
import { Row } from "../ui/Row";

const COPIED_MS = 1_500;
const DROP = 140;

/** §4.3 item 6: the Drop flies here from the logo, big; who it is; then version, system, links. */
export function About() {
  const t = useT();
  const [info, setInfo] = useState<{ version: string; os: string } | null>(null);
  const [copied, setCopied] = useState(false);
  const [update, setUpdate] = useState<UpdateState>({ kind: "idle" });
  const timer = useRef<ReturnType<typeof setTimeout>>(undefined);

  useEffect(() => {
    ipc.appInfo().then(setInfo).catch(() => {});
    ipc.updateState().then(setUpdate).catch(() => {});
    const un = events.update(setUpdate);
    return () => {
      clearTimeout(timer.current);
      un.then((f) => f());
    };
  }, []);

  const copyDiagnostics = async () => {
    const report = await ipc.copyDiagnostics();
    await ipc.copyText(report);
    setCopied(true);
    clearTimeout(timer.current);
    timer.current = setTimeout(() => setCopied(false), COPIED_MS);
  };

  return (
    <section className="mx-auto max-w-[640px] px-10 pt-14 pb-10">
      <div className="flex flex-col items-center text-center">
        <button
          type="button"
          data-drop-slot="about"
          aria-label={t("about.drop_poke")}
          onClick={() => useDropStage.getState().celebrate()}
          className="cursor-pointer rounded-full"
          style={{ width: DROP, height: Math.round((DROP * 50) / 48) }}
        />
        <div data-title className="mt-5 overflow-hidden pb-1">
          <h1 className="m-0 text-[28px] leading-[1.1] font-[450] tracking-[-0.02em]">{t("about.drop_title")}</h1>
        </div>
        <p className="mt-2 mb-0 max-w-[440px] leading-relaxed text-pretty text-text-secondary">{t("about.drop_body")}</p>
      </div>
      <div className="mt-10">
        <Row label={t("about.version")}>
          <span className="font-mono text-[13px] text-text-secondary tabular-nums">{info?.version ?? "—"}</span>
        </Row>
        <Row label={t("about.os")}>
          <span className="font-mono text-[13px] text-text-secondary">{info?.os ?? "—"}</span>
        </Row>
        <Row label={t("about.updates")} description={updateText(update, t)}>
          {update.kind === "available" ? (
            <Button onClick={() => ipc.installUpdate().catch(() => {})}>{t("about.install")}</Button>
          ) : update.kind === "idle" || update.kind === "up_to_date" || update.kind === "failed" ? (
            <Button onClick={() => ipc.checkUpdates().catch(() => {})}>{t("about.check")}</Button>
          ) : null}
        </Row>
        <Row label={t("about.diagnostics")} description={t("about.diagnostics_hint")}>
          <Button onClick={copyDiagnostics}>
            {copied ? <Check data-accent size={14} className="text-success" aria-hidden /> : null}
            {copied ? t("common.copied") : t("about.copy")}
          </Button>
        </Row>
        <div className="row flex gap-5 border-border py-3.5">
          {(["github", "releases"] as const).map((l) => (
            <button key={l} type="button" onClick={() => ipc.openLink(l).catch(() => {})} className="link flex items-center gap-1 text-text-secondary">
              {t(`about.${l}`)}
              <ArrowUpRight size={13} aria-hidden />
            </button>
          ))}
        </div>
      </div>
    </section>
  );
}

/** The line under "Updates": what the updater is doing or found; a failed check is said calmly. */
function updateText(u: UpdateState, t: ReturnType<typeof useT>): string | undefined {
  switch (u.kind) {
    case "checking":
      return t("about.checking");
    case "up_to_date":
      return t("about.up_to_date");
    case "available":
      return t("about.available", { version: u.version });
    case "downloading":
      return t("about.downloading", { n: u.percent });
    case "ready":
      return t("about.ready");
    case "failed":
      return t("about.update_failed");
    default:
      return undefined;
  }
}
