// Temporary Russian copy for Plan 1; Plan 2 adds full RU/EN i18n.

export const overlayText = {
  dictation: "Диктовка",
  translate: { Business: "→ EN · Деловой", Casual: "→ EN · Разговорный" },
  processing: "Обработка…",
  slow: "Жду ответа сервера…",
  pending: (n: number) => `ещё ${n} в обработке`,
  done: {
    pasted: "Вставлено",
    typed: "Напечатано",
    copied: "Скопировано — нажми ⌘V",
    failed: "Не удалось скопировать",
  },
  degraded: "Вставлено без обработки",
  errors: {
    no_key: "Добавь API-ключ",
    no_accessibility: "Нужен доступ Accessibility",
    no_microphone: "Микрофон не найден",
    no_microphone_access: "Нет доступа к микрофону — открой Настройки",
    nothing_heard: "Ничего не услышал",
    network: "Нет соединения",
    bad_key: "Ключ не подходит — проверь в Настройках",
    rate_limited: (m: number | null) =>
      m ? `Лимит исчерпан, сбросится через ${m} мин` : "Лимит исчерпан, попробуй позже",
    service_down: "Сервис недоступен",
  } as Record<string, string | ((m: number | null) => string)>,
};

export function errorText(key: string, minutes: number | null): string {
  const v = overlayText.errors[key];
  if (typeof v === "function") return v(minutes);
  return v ?? key;
}
