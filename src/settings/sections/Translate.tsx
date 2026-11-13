import { useT } from "../../i18n";
import { useApp } from "../../lib/store";
import type { TranslateStyle } from "../../lib/types";
import { Row } from "../ui/Row";
import { Segmented } from "../ui/Segmented";
import { Section } from "./Section";

const STYLES: { value: TranslateStyle; label: string; hint: string; example: string }[] = [
  { value: "Business", label: "translate.business", hint: "translate.business_hint", example: "translate.example_business" },
  { value: "Casual", label: "translate.casual", hint: "translate.casual_hint", example: "translate.example_casual" },
];

export function Translate() {
  const t = useT();
  const s = useApp((a) => a.settings);
  const update = useApp((a) => a.update);
  if (!s) return null;

  return (
    <Section route="translate" subtitle={t("translate.subtitle")}>
      <Row label={t("translate.style")}>
        <Segmented<TranslateStyle>
          label={t("translate.style")}
          value={s.translate_style}
          onChange={(v) => update({ translate_style: v })}
          options={STYLES.map((x) => ({ value: x.value, label: t(x.label) }))}
        />
      </Row>
      <div className="mt-8 text-[13px] text-text-secondary">{t("translate.example_title")}</div>
      <p className="mt-1.5 mb-4 text-text">«{t("translate.example_source")}»</p>
      <div className="grid grid-cols-2 gap-3">
        {STYLES.map((x) => {
          const on = s.translate_style === x.value;
          return (
            <button
              key={x.value}
              type="button"
              data-selected={on ? "" : undefined}
              data-accent={on ? "" : undefined}
              onClick={() => !on && update({ translate_style: x.value })}
              className={`style-card rounded-xl border p-4 text-left ${on ? "border-translate" : "border-border"}`}
            >
              <div className="text-[13px] text-text-secondary">
                {t(x.label)} · {t(x.hint)}
              </div>
              <div className="mt-2 leading-normal text-text">{t(x.example)}</div>
            </button>
          );
        })}
      </div>
      <p className="mt-6 text-[13px] leading-normal text-text-secondary">{t("translate.wait_note")}</p>
    </Section>
  );
}
