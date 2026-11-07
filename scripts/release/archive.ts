// Reads an update archive's entry names the way the installed Utter will — not through macOS tar,
// which quietly turns AppleDouble "._*" entries back into file attributes (final review C1).
import { gunzipSync } from "node:zlib";

const text = (b: Buffer) => b.toString("utf8").replace(/\0.*$/s, "");

/** Entry names of a .tar.gz (ustar, with GNU long names and pax `path` records). */
export function listTarGz(gz: Buffer): string[] {
  const tar = gunzipSync(gz);
  const names: string[] = [];
  let longName: string | null = null;
  for (let off = 0; off + 512 <= tar.length; ) {
    const h = tar.subarray(off, off + 512);
    if (h.every((b) => b === 0)) break;
    const size = parseInt(text(h.subarray(124, 136)).trim() || "0", 8);
    const type = String.fromCharCode(h[156] || 48);
    const data = tar.subarray(off + 512, off + 512 + size);
    off += 512 + Math.ceil(size / 512) * 512;
    if (type === "L") {
      longName = text(data);
      continue;
    }
    if (type === "x" || type === "g") {
      const path = /\d+ path=([^\n]*)\n/.exec(data.toString("utf8"))?.[1];
      if (type === "x" && path) longName = path;
      continue;
    }
    const prefix = text(h.subarray(257, 263)).startsWith("ustar") ? text(h.subarray(345, 500)) : "";
    names.push(longName ?? (prefix ? `${prefix}/${text(h.subarray(0, 100))}` : text(h.subarray(0, 100))));
    longName = null;
  }
  return names;
}

/** The updater drops the first path component and unpacks the rest into the app: only Utter.app may be there. */
export function archiveProblems(names: string[]): string[] {
  if (names.length === 0) return ["Архив обновления пуст."];
  const out: string[] = [];
  for (const n of names) {
    const base = n.replace(/\/$/, "").split("/").pop() ?? "";
    if (base.startsWith("._")) out.push(`В архиве служебный файл macOS «${n}» — установленный Utter не сможет обновиться.`);
    else if (n !== "Utter.app" && n !== "Utter.app/" && !n.startsWith("Utter.app/")) out.push(`В архиве лишнее «${n}» — там должно быть только Utter.app.`);
  }
  return out;
}
