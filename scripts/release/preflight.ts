// Everything that must be true before a release is built and published; problems in plain words.
import { isNewer } from "./version.ts";

export interface Facts {
  branch: string;
  dirty: boolean;
  head: string;
  originMain: string;
  tagLocal: boolean;
  tagRemote: boolean;
  current: string;
  next: string;
  notes: string | null;
  identity: boolean;
  updaterKey: boolean;
  targets: string[];
}


export function preflight(f: Facts): string[] {
  const out: string[] = [];
  const tag = `v${f.next}`;
  if (f.branch !== "main") out.push(`Выпуск делается с ветки main, а сейчас открыта «${f.branch}».`);
  if (f.dirty) out.push("Есть несохранённые изменения — сначала сохраните их (commit) или уберите.");
  if (f.head !== f.originMain) out.push("main на этом Mac не совпадает с main на GitHub — сначала git pull / git push.");
  if (f.tagLocal) out.push(`Метка ${tag} уже есть на этом Mac — такая версия уже выпускалась.`);
  if (f.tagRemote)
    out.push(`Метка ${tag} уже есть на GitHub. Если выпуск оборвался на загрузке файлов, догрузите их: gh release upload ${tag} dist/${tag}/*`);
  if (!isNewer(f.next, f.current)) out.push(`Версия ${f.next} должна быть больше текущей ${f.current}.`);
  if (f.notes === null || f.notes.trim() === "") out.push(`Нет заметок к выпуску: создайте docs/releases/${f.next}.md (что нового).`);
  if (!f.identity) out.push("На этом Mac нет сертификата «Utter Dev» для подписи (Связка ключей → Мои сертификаты).");
  if (!f.updaterKey) out.push("Нет ключа обновлений ~/.tauri/utter.key (копия — в «Паролях», «utter - key»).");
  if (!f.targets.includes("x86_64-apple-darwin")) out.push("Нет сборки для Intel: выполните rustup target add x86_64-apple-darwin");
  if (!f.targets.includes("aarch64-apple-darwin")) out.push("Нет сборки для Apple Silicon: выполните rustup target add aarch64-apple-darwin");
  return out;
}
