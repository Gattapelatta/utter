import { useT } from "../../i18n";
import { useApp } from "../../lib/store";
import type { InsertMethod, RecordMode, Theme, UiLanguage } from "../../lib/types";
import { MAIN_LANGUAGE_COUNT, SPEECH_LANGUAGES } from "../languages";
import { Row } from "../ui/Row";
import { Segmented } from "../ui/Segmented";
import { Select } from "../ui/Select";
import { Switch } from "../ui/Switch";
import { Section } from "./Section";
import { Usage } from "./Usage";

const AUTO = "auto";

export function General() {
  const t = useT();
  const s = useApp((a) => a.settings);
  const update = useApp((a) => a.update);
  if (!s) return null;
  const lang = (l: (typeof SPEECH_LANGUAGES)[number]) => ({ value: l.code ?? AUTO, label: l.code ? l.name : t("general.auto") });

  return (
    <Section route="general">
      <Row label={t("general.record_mode")} description={t("general.record_mode_hint")}>
        <Segmented<RecordMode>
          label={t("general.record_mode")}
          value={s.record_mode}
          onChange={(v) => update({ record_mode: v })}
          options={[
            { value: "Hold", label: t("general.hold") },
            { value: "Toggle", label: t("general.toggle") },
          ]}
        />
      </Row>
      <Row label={t("general.insert_method")} description={t("general.insert_method_hint")}>
        <Segmented<InsertMethod>
          label={t("general.insert_method")}
          value={s.insert_method}
          onChange={(v) => update({ insert_method: v })}
          options={[
            { value: "Paste", label: t("general.paste") },
            { value: "Type", label: t("general.type") },
          ]}
        />
      </Row>
      <Row label={t("general.speech_language")} description={t("general.speech_language_hint")}>
        <Select
          label={t("general.speech_language")}
          value={s.speech_language ?? AUTO}
          onChange={(v) => update({ speech_language: v === AUTO ? null : v })}
          options={SPEECH_LANGUAGES.slice(0, MAIN_LANGUAGE_COUNT).map(lang)}
          groups={[{ label: t("general.other_languages"), options: SPEECH_LANGUAGES.slice(MAIN_LANGUAGE_COUNT).map(lang) }]}
        />
      </Row>
      <Row label={t("general.cleanup")} description={t("general.cleanup_hint")}>
        <Switch label={t("general.cleanup")} checked={s.cleanup} onChange={(v) => update({ cleanup: v })} />
      </Row>
      <Row label={t("general.paragraphs")} description={t("general.paragraphs_hint")}>
        <Switch label={t("general.paragraphs")} checked={s.paragraphs} onChange={(v) => update({ paragraphs: v })} />
      </Row>
      <Row label={t("general.autostart")}>
        <Switch label={t("general.autostart")} checked={s.autostart} onChange={(v) => update({ autostart: v })} />
      </Row>
      <Row label={t("general.notch_drop")} description={t("general.notch_drop_hint")}>
        <Switch label={t("general.notch_drop")} checked={s.notch_drop} onChange={(v) => update({ notch_drop: v })} />
      </Row>
      <Row label={t("general.theme")}>
        <Segmented<Theme>
          label={t("general.theme")}
          value={s.theme}
          onChange={(v) => update({ theme: v })}
          options={[
            { value: "Dark", label: t("general.theme_dark") },
            { value: "Light", label: t("general.theme_light") },
            { value: "System", label: t("general.theme_system") },
          ]}
        />
      </Row>
      <Row label={t("general.ui_language")}>
        <Segmented<UiLanguage>
          label={t("general.ui_language")}
          value={s.ui_language}
          onChange={(v) => update({ ui_language: v })}
          options={[
            { value: "Auto", label: t("general.lang_auto") },
            { value: "Ru", label: "Русский" },
            { value: "En", label: "English" },
          ]}
        />
      </Row>
      <Usage />
    </Section>
  );
}
