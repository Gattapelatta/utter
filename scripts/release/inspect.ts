// Reading what `codesign -dv` and `lipo -archs` say about a built app.

/** `codesign -dv` writes to stderr; an ad-hoc signature has no Authority line. */
export function parseCodesign(out: string): { authority: string | null; identifier: string | null } {
  const field = (name: string) => out.match(new RegExp(`^${name}=(.+)$`, "m"))?.[1].trim() ?? null;
  return { authority: field("Authority"), identifier: field("Identifier") };
}

export function parseLipo(out: string): string[] {
  return out.trim().split(/\s+/).filter(Boolean);
}

/** The certificate leaf hash in `codesign -d -r-` output — what users' permissions are tied to. */
export function parseLeaf(out: string): string | null {
  return /certificate leaf = H"([0-9a-f]+)"/.exec(out)?.[1] ?? null;
}

/** The README promises macOS 13+; the app's Info.plist must say so, or an older Mac installs it
 *  and fails instead of macOS refusing up front. `value` is LSMinimumSystemVersion (null = absent). */
export const MIN_MACOS = "13.0";
export function minimumSystemProblem(value: string | null): string | null {
  if (value === null) return `В приложении не указана минимальная macOS (нужна ${MIN_MACOS}).`;
  return value.trim() === MIN_MACOS ? null : `Приложение пускает на macOS ${value.trim()}, а работает с ${MIN_MACOS}.`;
}

/** Agents run `Contents/MacOS/utter-agent`; `archs` is its `lipo -archs` (null = the file is missing). */
export function helperProblem(archs: string[] | null): string | null {
  if (archs === null) return "В приложении нет utter-agent (Капля не услышит агентов).";
  const missing = ["x86_64", "arm64"].filter((a) => !archs.includes(a));
  return missing.length ? `В utter-agent нет архитектуры ${missing.join(" и ")}.` : null;
}
