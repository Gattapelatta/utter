import { motion, useReducedMotion } from "motion/react";
import { useT } from "../i18n";
import type { OverlayState } from "../lib/types";
import { Check } from "./Check";
import { shake, widthSpring } from "./pillMotion";
import { Wave } from "./Wave";

interface Props {
  state: OverlayState;
  /** Recent input levels, oldest first, 0..1. */
  levels: number[];
  now: number;
}

/** After this long without an answer the network is the likely culprit. */
const SLOW_AFTER_MS = 4_000;

function formatTimer(ms: number): string {
  const s = Math.max(0, Math.floor(ms / 1000));
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
}

type Tone = "record" | "translate" | "success" | "danger";

const ACCENT: Record<Tone, string> = {
  record: "var(--accent-record)",
  translate: "var(--accent-translate)",
  success: "var(--accent-success)",
  danger: "var(--accent-danger)",
};

/** The recording pill (§4.2). Always dark: `data-theme="dark"` pins the tokens. */
export function Pill({ state, levels, now }: Props) {
  const t = useT();
  const reduced = useReducedMotion() ?? false;
  if (state.kind === "hidden" || state.kind === "cancelled") return null;

  let tone: Tone = "record";
  let body: React.ReactNode;
  let label: string;

  if (state.kind === "recording") {
    const translate = state.mode === "Translate";
    tone = translate ? "translate" : "record";
    label = translate ? t(state.style === "Casual" ? "pill.translate_casual" : "pill.translate_business") : t("pill.dictation");
    body = (
      <>
        <Wave levels={levels} reduced={reduced} />
        <Label text={label} accent />
        <motion.span
          layout="position"
          className="font-mono text-[13px] tabular-nums text-text-secondary data-[warn]:text-warning"
          {...(state.warn ? { "data-warn": "", "data-accent": "" } : {})}
        >
          {formatTimer(now - state.started_at_ms)}
        </motion.span>
        {state.pending > 0 && (
          <motion.span layout="position" className="shrink-0 text-[11px] text-text-secondary">
            {t("pill.pending", { n: state.pending })}
          </motion.span>
        )}
      </>
    );
  } else if (state.kind === "processing") {
    label = now - state.since_ms > SLOW_AFTER_MS ? t("pill.slow") : t("pill.processing");
    body = (
      <>
        <motion.span
          aria-hidden
          className="pill-shimmer h-[2px] w-[56px] shrink-0 rounded-full"
          initial={reduced ? { opacity: 0 } : { transform: "scaleY(8)", opacity: 0.4 }}
          animate={reduced ? { opacity: 1 } : { transform: "scaleY(1)", opacity: 1 }}
          transition={{ duration: reduced ? 0.15 : 0.2, ease: [0.23, 1, 0.32, 1] }}
        />
        <Label text={label} />
      </>
    );
  } else if (state.kind === "done") {
    tone = state.outcome === "failed" ? "danger" : "success";
    label = state.degraded ? t("pill.degraded") : t(`pill.${state.outcome}`);
    body = (
      <>
        {state.outcome === "failed" ? <Dot /> : <Check reduced={reduced} />}
        <Label text={label} />
      </>
    );
  } else {
    tone = "danger";
    const key =
      state.message_key === "rate_limited" && state.minutes === null ? "errors.rate_limited_later" : `errors.${state.message_key}`;
    label = t(key, { provider: state.provider ?? "", n: state.minutes ?? 0 });
    body = (
      <motion.div
        key={state.message_key}
        className="flex min-w-0 items-center gap-2.5"
        initial={reduced ? false : { transform: "translateX(-4px)" }}
        animate={{ transform: "translateX(0px)" }}
        transition={shake}
      >
        <Dot />
        <Label text={label} />
      </motion.div>
    );
  }

  return (
    <motion.div
      data-pill
      data-theme="dark"
      data-tone={tone}
      data-clickable={state.kind === "error" && state.action ? "" : undefined}
      layout
      transition={{ layout: reduced ? { duration: 0 } : widthSpring }}
      style={{ borderRadius: 22, ["--accent" as string]: ACCENT[tone] }}
      className="pill relative flex h-11 max-w-[448px] items-center gap-2.5 px-4 font-sans text-[13px] text-text"
    >
      {body}
    </motion.div>
  );
}

function Label({ text, accent }: { text: string; accent?: boolean }) {
  return (
    <motion.span layout="position" data-label className={`min-w-0 truncate ${accent ? "font-medium" : ""}`}>
      {text}
    </motion.span>
  );
}

function Dot() {
  return <span data-accent aria-hidden className="size-2 shrink-0 rounded-full bg-[var(--accent)]" />;
}
