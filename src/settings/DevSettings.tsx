// Plain service page for Plan 1. Plan 2 replaces it with the designed settings window.
import { useCallback, useEffect, useState } from "react";
import { ipc } from "../lib/ipc";
import {
  keyLabel,
  type HistoryEntry,
  type PermissionsStatus,
  type ProviderKind,
  type Settings,
  type UsageDay,
} from "../lib/types";
import "./dev.css";

const LANGUAGES: [string | null, string][] = [
  [null, "Авто"],
  ["ru", "Русский"],
  ["en", "English"],
  ["uk", "Українська"],
  ["de", "Deutsch"],
  ["es", "Español"],
  ["fr", "Français"],
];

export function DevSettings() {
  const [s, setS] = useState<Settings | null>(null);
  const [keyInput, setKeyInput] = useState("");
  const [mask, setMask] = useState<string | null>(null);
  const [verify, setVerify] = useState<string>("");
  const [perms, setPerms] = useState<PermissionsStatus | null>(null);
  const [history, setHistory] = useState<HistoryEntry[]>([]);
  const [usage, setUsage] = useState<UsageDay | null>(null);
  const [capturing, setCapturing] = useState<"dictation" | "translate" | null>(null);
  const [note, setNote] = useState("");

  const refresh = useCallback(async () => {
    const settings = await ipc.getSettings();
    setS(settings);
    setMask(await ipc.apiKeyMask(settings.provider));
    setPerms(await ipc.permissionsStatus());
    setHistory((await ipc.historyList()).slice(0, 10));
    setUsage(await ipc.usageToday());
  }, []);

  useEffect(() => {
    refresh();
    const t = setInterval(async () => {
      setPerms(await ipc.permissionsStatus());
      setHistory((await ipc.historyList()).slice(0, 10));
      setUsage(await ipc.usageToday());
    }, 1000);
    return () => clearInterval(t);
  }, [refresh]);

  if (!s) return null;

  const update = async (patch: Partial<Settings>) => {
    const next = { ...s, ...patch };
    setS(next);
    await ipc.saveSettings(next);
    if (patch.provider) setMask(await ipc.apiKeyMask(patch.provider));
  };

  const saveKey = async () => {
    await ipc.setApiKey(s.provider, keyInput);
    setKeyInput("");
    setMask(await ipc.apiKeyMask(s.provider));
    setVerify("");
  };

  const runVerify = async () => {
    setVerify("…");
    try {
      await ipc.verifyKey(s.provider);
      setVerify("✓ ключ работает");
    } catch (e) {
      setVerify(`✗ ${e}`);
    }
  };

  const capture = async (which: "dictation" | "translate") => {
    setCapturing(which);
    try {
      const key = await ipc.captureKey();
      const other = which === "dictation" ? s.hotkeys.translate : s.hotkeys.dictation;
      if (JSON.stringify(other) === JSON.stringify(key)) {
        setNote("Эта клавиша уже занята другим режимом");
      } else {
        await update({ hotkeys: { ...s.hotkeys, [which]: key } });
        setNote("");
      }
    } catch (e) {
      setNote(`Назначение отменено: ${e}`);
    } finally {
      setCapturing(null);
    }
  };

  return (
    <main className="dev">
      <h1>Utter</h1>
      <p className="muted">Служебная страница настроек — красивый интерфейс появится в Плане 2.</p>

      <section>
        <h2>Разрешения</h2>
        <div className="row">
          <span>Микрофон: {perms?.microphone === "Granted" ? "✓ выдано" : perms?.microphone}</span>
          {perms?.microphone !== "Granted" && <button onClick={() => ipc.requestMicrophone()}>Разрешить</button>}
        </div>
        <div className="row">
          <span>Accessibility: {perms?.accessibility ? "✓ выдано" : "нет"}</span>
          {!perms?.accessibility && <button onClick={() => ipc.openAccessibilitySettings()}>Открыть настройки</button>}
        </div>
        <div className="row">
          <span>Горячие клавиши: {perms?.hotkeys_running ? "✓ работают" : "ожидают разрешения"}</span>
        </div>
      </section>

      <section>
        <h2>Провайдер</h2>
        <div className="row">
          <select value={s.provider} onChange={(e) => update({ provider: e.target.value as ProviderKind })}>
            <option value="Groq">Groq (бесплатно)</option>
            <option value="OpenAi">OpenAI</option>
          </select>
          <span className="muted">Ключ: {mask ?? "не задан"}</span>
        </div>
        <div className="row">
          <input type="password" placeholder="Вставь API-ключ" value={keyInput} onChange={(e) => setKeyInput(e.target.value)} />
          <button onClick={saveKey} disabled={!keyInput.trim()}>Сохранить</button>
          <button onClick={runVerify} disabled={!mask}>Проверить</button>
          <span>{verify}</span>
        </div>
        <p className="muted">
          Бесплатный ключ Groq: console.groq.com → API Keys → Create API Key.
        </p>
      </section>

      <section>
        <h2>Клавиши</h2>
        <div className="row">
          <span>Диктовка: <b>{keyLabel(s.hotkeys.dictation)}</b></span>
          <button onClick={() => capture("dictation")} disabled={capturing !== null}>
            {capturing === "dictation" ? "Нажми клавишу… (Esc — отмена)" : "Назначить"}
          </button>
        </div>
        <div className="row">
          <span>Перевод: <b>{keyLabel(s.hotkeys.translate)}</b></span>
          <button onClick={() => capture("translate")} disabled={capturing !== null}>
            {capturing === "translate" ? "Нажми клавишу… (Esc — отмена)" : "Назначить"}
          </button>
        </div>
        {note && <p className="muted">{note}</p>}
      </section>

      <section>
        <h2>Основное</h2>
        <label className="row">
          Режим записи
          <select value={s.record_mode} onChange={(e) => update({ record_mode: e.target.value as Settings["record_mode"] })}>
            <option value="Hold">Удержание</option>
            <option value="Toggle">Нажатие</option>
          </select>
        </label>
        <label className="row">
          Способ вставки
          <select value={s.insert_method} onChange={(e) => update({ insert_method: e.target.value as Settings["insert_method"] })}>
            <option value="Paste">Вставка</option>
            <option value="Type">Печать</option>
          </select>
        </label>
        <label className="row">
          Язык речи
          <select
            value={s.speech_language ?? ""}
            onChange={(e) => update({ speech_language: e.target.value || null })}
          >
            {LANGUAGES.map(([code, name]) => (
              <option key={name} value={code ?? ""}>{name}</option>
            ))}
          </select>
        </label>
        <label className="row">
          <input type="checkbox" checked={s.cleanup} onChange={(e) => update({ cleanup: e.target.checked })} />
          Чистовая обработка
        </label>
        <label className="row">
          <input type="checkbox" checked={s.paragraphs} onChange={(e) => update({ paragraphs: e.target.checked })} />
          Абзацы в длинных текстах
        </label>
        <label className="row">
          Стиль перевода
          <select value={s.translate_style} onChange={(e) => update({ translate_style: e.target.value as Settings["translate_style"] })}>
            <option value="Business">Деловой</option>
            <option value="Casual">Разговорный</option>
          </select>
        </label>
      </section>

      <section>
        <h2>Сегодня</h2>
        <p className="muted">
          {usage
            ? `${usage.stt_requests} записей · ${Math.round(usage.audio_secs)} с аудио · ${usage.llm_requests} обработок · ${usage.llm_tokens} токенов`
            : "—"}
        </p>
      </section>

      <section>
        <h2>История</h2>
        {history.length === 0 && <p className="muted">Пока пусто</p>}
        {history.map((h) => (
          <div className="row history" key={h.id}>
            <span className="muted">{new Date(h.created_at_ms).toLocaleTimeString()}</span>
            <span className="muted">{h.mode === "Translate" ? "→ EN" : "Диктовка"}</span>
            <span className="grow">{h.text || (h.status.kind === "failed" ? `Ошибка: ${h.status.message}` : "—")}</span>
            {h.status.kind === "failed" && h.audio_file && <button onClick={() => ipc.historyRetry(h.id)}>Повторить</button>}
            {h.text && <button onClick={() => navigator.clipboard.writeText(h.text)}>Копировать</button>}
          </div>
        ))}
      </section>

      <section>
        <button onClick={async () => navigator.clipboard.writeText(await ipc.copyDiagnostics())}>
          Скопировать диагностику
        </button>
      </section>
    </main>
  );
}
