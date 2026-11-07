// Version numbers of a release: what the user types, and where the app keeps its own version.

const SEMVER = /^v?(\d+)\.(\d+)\.(\d+)$/;

/** "0.2.0" or "v0.2.0" → "0.2.0"; anything else is refused with an example. */
export function parseVersion(input: string): string {
  const m = SEMVER.exec(input.trim());
  if (!m) throw new Error(`Нужна версия вида 0.2.0, а не «${input.trim()}»`);
  return `${Number(m[1])}.${Number(m[2])}.${Number(m[3])}`;
}

function parts(v: string): number[] {
  return parseVersion(v).split(".").map(Number);
}

/** Whether `next` is a later version than `current` (by numbers, so 0.10.0 > 0.9.9). */
export function isNewer(next: string, current: string): boolean {
  const [a, b] = [parts(next), parts(current)];
  for (let i = 0; i < 3; i++) if (a[i] !== b[i]) return a[i] > b[i];
  return false;
}

export interface VersionFiles {
  tauriConf: string;
  cargoToml: string;
  packageJson: string;
}

/** The app's version in its three files; dependencies' versions are left alone. */
export function setVersion(files: VersionFiles, v: string): VersionFiles {
  const json = (text: string) => text.replace(/^(\s*"version"\s*:\s*)"[^"]*"/m, `$1"${v}"`);
  const cargo = files.cargoToml.replace(/(\[package\][^[]*?\nversion\s*=\s*)"[^"]*"/, `$1"${v}"`);
  return { tauriConf: json(files.tauriConf), cargoToml: cargo, packageJson: json(files.packageJson) };
}
