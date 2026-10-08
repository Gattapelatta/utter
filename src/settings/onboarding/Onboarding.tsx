import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { motion, type Variants } from "motion/react";
import { useT } from "../../i18n";
import { ipc } from "../../lib/ipc";
import { useApp } from "../../lib/store";
import type { PermissionsStatus } from "../../lib/types";
import { HotkeyRows } from "../HotkeyRows";
import { keyLabel } from "../keys";
import { Button } from "../ui/Button";
import { DemoPhrase } from "./DemoPhrase";
import { useDropStage } from "../drop/dropStage";
import { Segmented } from "../ui/Segmented";
import type { Theme } from "../../lib/types";
import { canAdvance, DROP_H, DROP_W, dropAt, dropY, firstIncompleteStep, GAP, STEPS, type StepId } from "./steps";
import { Accessibility } from "./steps/Accessibility";
import { Microphone } from "./steps/Microphone";
import { ProviderKey } from "./steps/ProviderKey";
import { Trial } from "./steps/Trial";

const FOOT = 84;
const POLL_MS = 1_000;
/** The Drop needs a moment to notice its new place before "settled" means "landed there". */
const LAND_GRACE_MS = 100;
const ADVANCE_AFTER_KEY_MS = 1_900;

const TITLES: Record<StepId, [string, string]> = {
  welcome: ["onboarding.welcome_title", "onboarding.welcome_body"],
  microphone: ["onboarding.mic_title", "onboarding.mic_body"],
  accessibility: ["onboarding.ax_title", "onboarding.ax_body"],
  provider: ["onboarding.provider_title", "onboarding.provider_body"],
  key: ["onboarding.keys_title", "onboarding.keys_body"],
  trial: ["onboarding.trial_title", "onboarding.trial_body"],
};

// One transform template per element (Motion cannot interpolate between different function lists).
const container: Variants = {
  hidden: {},
  in: { transition: { staggerChildren: 0.06 } },
  out: { transition: { staggerChildren: 0.025 } },
};
const title: Variants = {
  hidden: { opacity: 0, transform: "translateY(44px)", filter: "blur(0px)" },
  in: { opacity: 1, transform: "translateY(0px)", filter: "blur(0px)", transition: { duration: 0.64, ease: [0.22, 1, 0.36, 1] } },
  out: { opacity: 0, transform: "translateY(-8px)", filter: "blur(4px)", transition: { duration: 0.2, ease: [0.4, 0, 1, 1] } },
};
const item: Variants = {
  hidden: { opacity: 0, transform: "translateY(10px) scale(0.985)", filter: "blur(6px)" },
  in: { opacity: 1, transform: "translateY(0px) scale(1)", filter: "blur(0px)", transition: { duration: 0.42, ease: [0.2, 0.9, 0.3, 1.15] } },
  out: { opacity: 0, transform: "translateY(-8px) scale(1)", filter: "blur(4px)", transition: { duration: 0.2, ease: [0.4, 0, 1, 1] } },
};

/** «Правый ⌥» reads «правый ⌥» inside a sentence; key names like «Caps Lock» or «F5» stay. */
const inSentence = (label: string) => label.replace(/^Правый /, "правый ");

function prefersReducedMotion(): boolean {
  return typeof matchMedia === "function" && matchMedia("(prefers-reduced-motion: reduce)").matches;
}

type Phase = "in" | "out" | "wait";

/** First run (§4.4): six steps with the living Drop, which moves between the centre and the top. */
export function Onboarding({ onDone }: { onDone: () => void }) {
  const t = useT();
  const settings = useApp((a) => a.settings);
  const update = useApp((a) => a.update);
  const reduced = useMemo(prefersReducedMotion, []);
  const [height, setHeight] = useState(() => window.innerHeight);
  const [perms, setPerms] = useState<PermissionsStatus | null>(null);
  const [step, setStep] = useState<StepId | null>(null);
  const [phase, setPhase] = useState<Phase>("in");
  const [keyOk, setKeyOk] = useState(false);
  const [finishing, setFinishing] = useState(false);

  const stepRef = useRef(step);
  const phaseRef = useRef(phase);
  const target = useRef<StepId | null>(null);
  const prevPerms = useRef<PermissionsStatus | null>(null);
  stepRef.current = step;
  phaseRef.current = phase;

  const yFor = useCallback(
    (s: StepId) => dropY(dropAt(s), height),
    [height],
  );

  const celebrate = useCallback(() => useDropStage.getState().celebrate(), []);

  const refresh = useCallback(() => {
    ipc
      .permissionsStatus()
      .then((p) => {
        const was = prevPerms.current;
        prevPerms.current = p;
        setPerms(p);
        const s = stepRef.current;
        if (was && s === "microphone" && was.microphone !== "Granted" && p.microphone === "Granted") celebrate();
        if (was && s === "accessibility" && !was.accessibility && p.accessibility) celebrate();
      })
      .catch(() => {});
  }, [celebrate]);

  // Resume where the user left off; then poll permissions once a second.
  useEffect(() => {
    const provider = useApp.getState().settings?.provider ?? "Gemini";
    Promise.all([ipc.permissionsStatus().catch(() => null), ipc.apiKeyMask(provider).catch(() => null)]).then(([status, mask]) => {
      // Unknown status: start from the beginning rather than show nothing.
      const p = status ?? { microphone: "NotDetermined" as const, accessibility: false, hotkeys_running: false };
      prevPerms.current = p;
      setPerms(p);
      setKeyOk(mask !== null);
      const first = firstIncompleteStep(p, mask !== null);
      ipc.uiLog("info", `onboarding resumes at ${first} (status ${status ? "read" : "unknown"}, key ${mask !== null})`).catch(() => {});
      setStep((s) => s ?? first);
    });
    const id = setInterval(refresh, POLL_MS);
    const resize = () => setHeight(window.innerHeight);
    window.addEventListener("resize", resize);
    return () => {
      clearInterval(id);
      window.removeEventListener("resize", resize);
    };
  }, [refresh]);

  // Text waits while the Drop flies to its new place, and appears once it has landed.
  useEffect(() => {
    if (phase !== "wait") return;
    const since = performance.now();
    let raf = 0;
    const check = (now: number) => {
      if (now - since > LAND_GRACE_MS && useDropStage.getState().settled) setPhase("in");
      else raf = requestAnimationFrame(check);
    };
    raf = requestAnimationFrame(check);
    return () => cancelAnimationFrame(raf);
  }, [phase]);

  const go = useCallback(
    (next: StepId) => {
      if (!stepRef.current || phaseRef.current !== "in" || next === stepRef.current) return;
      if (reduced) {
        setStep(next);
        return;
      }
      target.current = next;
      setPhase("out");
    },
    [reduced],
  );

  /** The old text has left: switch the step (the Drop flies if its place changes), or finish. */
  const advance = async () => {
    if (finishing) {
      await update({ onboarding_done: true, autostart: true });
      onDone();
      return;
    }
    const next = target.current;
    const cur = stepRef.current;
    if (!next || !cur) return;
    target.current = null;
    setPhase(dropAt(next) !== dropAt(cur) ? "wait" : "in");
    setStep(next);
  };

  /** The text and the footer leave; then the app takes over and the Drop flies to the logo. */
  const finish = async () => {
    if (phaseRef.current !== "in") return;
    if (reduced) {
      await update({ onboarding_done: true, autostart: true });
      onDone();
      return;
    }
    setFinishing(true);
    setPhase("out");
  };

  const onKeyVerified = useCallback(
    (checked: boolean) => {
      setKeyOk(true);
      if (!checked) return;
      celebrate();
      setTimeout(() => {
        if (stepRef.current === "provider") go("key");
      }, ADVANCE_AFTER_KEY_MS);
    },
    [celebrate, go],
  );

  if (!step || !settings) return null;
  const index = STEPS.indexOf(step);
  const [titleKey, bodyKey] = TITLES[step];

  const content = (() => {
    switch (step) {
      case "welcome":
        return (
          <>
            <DemoPhrase reduced={reduced} />
            <div className="mt-5 flex justify-center">
              <Segmented<Theme>
                label={t("general.theme")}
                value={settings.theme === "Light" ? "Light" : "Dark"}
                onChange={(v) => update({ theme: v })}
                options={[
                  { value: "Dark", label: t("general.theme_dark") },
                  { value: "Light", label: t("general.theme_light") },
                ]}
              />
            </div>
          </>
        );
      case "microphone":
        return <Microphone perms={perms} refresh={refresh} />;
      case "accessibility":
        return <Accessibility perms={perms} />;
      case "provider":
        return <ProviderKey provider={settings.provider} ok={keyOk} onVerified={onKeyVerified} />;
      case "key":
        return (
          <div className="mx-auto w-[440px] text-left">
            <HotkeyRows hints={false} translateLabel={t("onboarding.keys_translate")} />
          </div>
        );
      case "trial":
        return <Trial onResult={celebrate} />;
    }
  })();

  const last = step === "trial";
  const skippable = step === "key" || step === "trial";
  const nextLabel = step === "welcome" ? t("onboarding.start") : last ? t("onboarding.finish") : t("common.next");

  return (
    <motion.div
      className="relative h-full overflow-hidden"
      initial={{ opacity: 0 }}
      animate={{ opacity: 1 }}
      transition={{ duration: reduced ? 0.15 : 1, ease: [0.22, 1, 0.36, 1] }}
    >
      {/* The Drop's place on this step; a new element per place, so the Drop flies between them. */}
      <div
        key={dropAt(step)}
        data-drop-slot="onboarding"
        className="absolute left-1/2"
        style={{ top: yFor(step), width: DROP_W, height: DROP_H, marginLeft: -DROP_W / 2 }}
      />

      <div
        data-stage
        data-phase={phase}
        className="absolute inset-x-0 top-0 flex flex-col items-center px-10 text-center"
        style={{ paddingTop: yFor(step) + DROP_H + GAP, bottom: FOOT }}
      >
        <motion.div
          key={step}
          className="flex w-full flex-col items-center"
          variants={container}
          initial="hidden"
          animate={phase === "in" ? "in" : phase === "out" ? "out" : "hidden"}
          onAnimationComplete={(d) => d === "out" && advance()}
        >
          <div className="overflow-hidden pb-1">
            <motion.h1 variants={title} className="m-0 text-[30px] leading-[1.15] font-[450] tracking-[-0.02em]">
              {t(titleKey)}
            </motion.h1>
          </div>
          <motion.p variants={item} className="mt-2 mb-6 text-[15px] text-text-secondary">
            {t(bodyKey, { key: inSentence(keyLabel(settings.hotkeys.dictation, t)) })}
          </motion.p>
          <motion.div variants={item} className="w-full">
            {content}
          </motion.div>
        </motion.div>
      </div>

      <motion.footer
        className="absolute inset-x-8 bottom-6 z-[2] flex items-center justify-between"
        animate={finishing ? { opacity: 0, transform: "translateY(8px)" } : { opacity: 1, transform: "translateY(0px)" }}
        transition={{ duration: 0.2, ease: [0.4, 0, 1, 1] }}
      >
        <div className="w-[200px]">
          {index > 0 && (
            <Button variant="ghost" onClick={() => go(STEPS[index - 1])}>
              {t("common.back")}
            </Button>
          )}
        </div>
        <div className="flex items-center gap-1.5" aria-hidden>
          {STEPS.map((s, i) => (
            <span key={s} className={`dot h-1.5 rounded-full ${i === index ? "w-[18px] bg-text" : "w-1.5 bg-surface-raised"}`} />
          ))}
        </div>
        <div className="flex w-[200px] justify-end gap-1">
          {skippable && (
            <Button variant="ghost" onClick={finish}>
              {t("common.skip")}
            </Button>
          )}
          <Button
            variant="primary"
            disabled={!canAdvance(step, perms, keyOk)}
            onClick={() => (last ? finish() : go(STEPS[index + 1]))}
          >
            {nextLabel}
          </Button>
        </div>
      </motion.footer>
    </motion.div>
  );
}
