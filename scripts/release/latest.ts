// latest.json: how every installed Utter learns about a new version (Tauri updater).

const REPO = "https://github.com/Gattapelatta/utter";

export interface LatestJson {
  version: string;
  notes: string;
  pub_date: string;
  platforms: Record<"darwin-aarch64" | "darwin-x86_64", { signature: string; url: string }>;
}

export function assetUrl(version: string, file: string): string {
  return `${REPO}/releases/download/v${version}/${file}`;
}

/** One universal archive serves both Apple Silicon and Intel Macs. */
export function buildLatestJson(a: { version: string; notes: string; pubDate: Date; signature: string; url: string }): LatestJson {
  const entry = { signature: a.signature, url: a.url };
  return {
    version: a.version,
    notes: a.notes,
    pub_date: a.pubDate.toISOString(),
    platforms: { "darwin-aarch64": entry, "darwin-x86_64": { ...entry } },
  };
}
