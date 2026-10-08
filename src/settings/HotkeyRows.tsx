import { useEffect, useRef, useState } from "react";
import { WarningCircle } from "@phosphor-icons/react";
import { useT } from "../i18n";
import { events, ipc } from "../lib/ipc";
import { useApp } from "../lib/store";
import type { Mode } from "../lib/types";
import { keyLabel, sameKey } from "./keys";
import { Button } from "./ui/Button";
import { Keycap } from "./ui/Keycap";
import { Row } from "./ui/Row";

const MODES: { mode: Mode; field: "dictation" | "translate"; label: string; hint: string }[] = [
  { mode: "Dictation", field: "dictation", label: "hotkeys.dictation", hint: "hotkeys.dictation_hint" },
  { mode: "Translate", field: "translate", label: "hotkeys.translate", hint: "hotkeys.translate_hint" },
];

/**
 * The two hotkey rows (Dictation, Translate) with capture, used by the Hotkeys section and
 * the onboarding keys step. `hints: false` hides the descriptions; `translateLabel` renames
 * the second row.
 */
export function HotkeyRows({ hints = true, translateLabel }: { hints?: boolean; translateLabel?: string }) {
  const t = useT();
  const s = useApp((a) => a.settings);
  const update = useApp((a) => a.update);
  const [capturing, setCapturing] = useState<Mode | null>(null);
  const [error, setError] = useState<{ mode: Mode; text: string } | null>(null);
  const [pressed, setPressed] = useState<Record<Mode, boolean>>({ Dictation: false, Translate: false });
  const live = useRef({ mounted: true, capturing: false });

  useEffect(() => {
    live.current.mounted = true;
    const un = events.hotkey(({ mode, down }) => setPressed((p) => ({ ...p, [mode]: down })));
    return () => {
      live.current.mounted = false;
      // Leaving mid-capture must hand the key back to dictation (Review Focus 3).
      if (live.current.capturing) ipc.cancelCapture().catch(() => {});
      un.then((f) => f());
    };
  }, []);

  if (!s) return null;

  const capture = async (mode: Mode, field: "dictation" | "translate") => {
    setError(null);
    setCapturing(mode);
    live.current.capturing = true;
    try {
      const key = await ipc.captureKey();
      if (!live.current.mounted) return;
      const other = MODES.find((m) => m.mode !== mode)!;
      const hotkeys = useApp.getState().settings?.hotkeys ?? s.hotkeys;
      if (sameKey(key, hotkeys[other.field])) {
        setError({ mode, text: t("errors.taken", { mode: t(other.label) }) });
      } else if (!sameKey(key, hotkeys[field])) {
        await update({ hotkeys: { ...hotkeys, [field]: key } });
      }
    } catch (e) {
      if (live.current.mounted && e !== "cancelled") setError({ mode, text: t(`errors.${String(e)}`) });
    } finally {
      live.current.capturing = false;
      if (live.current.mounted) setCapturing(null);
    }
  };

  return (
    <>
      {MODES.map(({ mode, field, label, hint }) => (
        <Row
          key={mode}
          label={mode === "Translate" && translateLabel ? translateLabel : t(label)}
          description={hints ? t(hint) : undefined}
          note={
            error?.mode === mode && (
              <div role="alert" className="mt-1.5 flex items-start gap-1.5 text-[13px] leading-snug text-text">
                <WarningCircle data-accent size={16} className="mt-px shrink-0 text-danger" aria-hidden />
                {error.text}
              </div>
            )
          }
        >
          <div className="flex shrink-0 items-center gap-3">
            {capturing === mode ? (
              <>
                <span className="text-[13px] text-text-secondary">
                  {t("hotkeys.capturing")} {t("hotkeys.esc_hint")}
                </span>
                <Button variant="ghost" onClick={() => ipc.cancelCapture().catch(() => {})}>
                  {t("common.cancel")}
                </Button>
              </>
            ) : (
              <>
                <Keycap label={keyLabel(s.hotkeys[field], t)} pressed={pressed[mode]} />
                <Button disabled={capturing !== null} onClick={() => capture(mode, field)}>
                  {t("common.change")}
                </Button>
              </>
            )}
          </div>
        </Row>
      ))}
    </>
  );
}
