import { useEffect, useState } from "react";
import { useT } from "../../i18n";

const PAIRS = 4;
const TYPE_MS = 35;
const PAUSE_MS = 600;
const SHOW_MS = 2600;
const FADE_MS = 250;

/** Step 1: a phrase "types" itself in the speaker's language, then turns into English (§4.4). */
export function DemoPhrase({ reduced }: { reduced: boolean }) {
  const t = useT();
  const [pair, setPair] = useState(1);
  const [typed, setTyped] = useState(0);
  const [phase, setPhase] = useState<"src" | "fade-src" | "dst" | "fade-dst">("src");
  const src = t(`onboarding.demo_${pair}_src`);
  const dst = t(`onboarding.demo_${pair}_dst`);

  useEffect(() => {
    if (reduced) return;
    let ms: number;
    let next: () => void;
    if (phase === "src" && typed < src.length) [ms, next] = [TYPE_MS, () => setTyped((n) => n + 1)];
    else if (phase === "src") [ms, next] = [PAUSE_MS, () => setPhase("fade-src")];
    else if (phase === "fade-src") [ms, next] = [FADE_MS, () => setPhase("dst")];
    else if (phase === "dst") [ms, next] = [SHOW_MS, () => setPhase("fade-dst")];
    else
      [ms, next] = [
        FADE_MS,
        () => {
          setPair((p) => (p % PAIRS) + 1);
          setTyped(0);
          setPhase("src");
        },
      ];
    const id = setTimeout(next, ms);
    return () => clearTimeout(id);
  }, [reduced, phase, typed, src.length]);

  const showDst = reduced || phase === "dst" || phase === "fade-dst";
  const fading = !reduced && (phase === "fade-src" || phase === "fade-dst");
  return (
    <p className="demo mx-auto mt-0 mb-0 min-h-[54px] text-balance max-w-[460px] text-[18px] leading-[1.45] text-text" style={{ opacity: fading ? 0 : 1 }}>
      <span className="mr-2 rounded border border-border px-1.5 py-px align-[2px] font-mono text-[11px] text-text-secondary">
        {showDst ? t("onboarding.demo_dst_tag") : t("onboarding.demo_src_tag")}
      </span>
      {showDst ? dst : src.slice(0, typed)}
    </p>
  );
}
