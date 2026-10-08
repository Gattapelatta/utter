import { useT } from "../../i18n";
import { HotkeyRows } from "../HotkeyRows";
import { Section } from "./Section";

export function Hotkeys() {
  const t = useT();
  return (
    <Section route="hotkeys" subtitle={t("hotkeys.subtitle")}>
      <HotkeyRows />
      <p className="mt-6 text-[13px] text-text-secondary">{t("hotkeys.cancel_hint")}</p>
    </Section>
  );
}
