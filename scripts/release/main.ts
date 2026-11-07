// Commands for scripts/release.sh (Node 24 runs this .ts directly). Logic lives in the tested modules.
//   facts <version> [--allow-branch]   — checks before a release; prints problems, exits 1 if any
//   bump <version>                     — writes the version into the app's three files
//   latest <version> <sig> <notes> <out> — writes latest.json
//   verify <app> <archive> <sig>       — signer, identifier, architectures, update signature
import { execFileSync } from "node:child_process";
import { existsSync, readFileSync, writeFileSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";
import { parseCodesign, parseLipo } from "./inspect.ts";
import { assetUrl, buildLatestJson } from "./latest.ts";
import { verifyMinisign } from "./minisign.ts";
import { preflight, type Facts } from "./preflight.ts";
import { parseVersion, setVersion } from "./version.ts";

const ROOT = new URL("../../", import.meta.url).pathname;
const FILES = {
  tauriConf: join(ROOT, "src-tauri/tauri.conf.json"),
  cargoToml: join(ROOT, "src-tauri/Cargo.toml"),
  packageJson: join(ROOT, "package.json"),
};
export const IDENTITY = "Utter Dev";
export const BUNDLE_ID = "app.utter.desktop";
export const ARCHIVE = "Utter.app.tar.gz";

function run(cmd: string, args: string[], opts: { stderr?: boolean } = {}): string {
  try {
    return execFileSync(cmd, args, { cwd: ROOT, encoding: "utf8", stdio: ["ignore", "pipe", opts.stderr ? "pipe" : "ignore"] });
  } catch (e) {
    const err = e as { stdout?: string; stderr?: string };
    if (opts.stderr) return `${err.stdout ?? ""}${err.stderr ?? ""}`;
    return "";
  }
}

/** `codesign -dvv` prints its details (Authority only from -vv) to stderr. */
function codesignDetails(app: string): string {
  return execFileSync("sh", ["-c", 'codesign -dvv "$0" 2>&1', app], { encoding: "utf8" });
}

function currentVersion(): string {
  return JSON.parse(readFileSync(FILES.tauriConf, "utf8")).version;
}

function facts(next: string, allowBranch: boolean): Facts {
  const notesPath = join(ROOT, "docs/releases", `${next}.md`);
  run("git", ["fetch", "--quiet", "origin", "--tags"]);
  const branch = run("git", ["rev-parse", "--abbrev-ref", "HEAD"]).trim();
  return {
    branch: allowBranch ? "main" : branch,
    dirty: run("git", ["status", "--porcelain"]).trim() !== "",
    head: run("git", ["rev-parse", "HEAD"]).trim(),
    originMain: allowBranch ? run("git", ["rev-parse", "HEAD"]).trim() : run("git", ["rev-parse", "origin/main"]).trim(),
    tagLocal: run("git", ["tag", "--list", `v${next}`]).trim() !== "",
    tagRemote: run("git", ["ls-remote", "--tags", "origin", `refs/tags/v${next}`]).trim() !== "",
    current: currentVersion(),
    next,
    notes: existsSync(notesPath) ? readFileSync(notesPath, "utf8") : null,
    identity: run("security", ["find-identity", "-p", "codesigning"]).includes(`"${IDENTITY}"`),
    updaterKey: existsSync(join(homedir(), ".tauri/utter.key")),
    targets: run("rustup", ["target", "list", "--installed"]).split("\n").map((s) => s.trim()).filter(Boolean),
  };
}

function verify(app: string, archive: string, sigPath: string): string[] {
  const problems: string[] = [];
  const cs = parseCodesign(codesignDetails(app));
  if (cs.authority !== IDENTITY) problems.push(`Приложение подписано «${cs.authority ?? "без сертификата"}», а нужно «${IDENTITY}».`);
  if (cs.identifier !== BUNDLE_ID) problems.push(`Идентификатор подписи «${cs.identifier}», а нужен ${BUNDLE_ID}.`);
  const archs = parseLipo(run("lipo", ["-archs", join(app, "Contents/MacOS/utter")]));
  if (!(archs.includes("x86_64") && archs.includes("arm64"))) problems.push(`В приложении архитектуры «${archs.join(" ")}», а нужны x86_64 и arm64.`);
  const pubkey = JSON.parse(readFileSync(FILES.tauriConf, "utf8")).plugins.updater.pubkey as string;
  if (!verifyMinisign(readFileSync(archive), readFileSync(sigPath, "utf8"), pubkey))
    problems.push("Подпись архива обновления не сходится с ключом в tauri.conf.json — люди не смогут обновиться.");
  return problems;
}

function main(argv: string[]): number {
  const [cmd, ...args] = argv;
  switch (cmd) {
    case "facts": {
      const next = parseVersion(args[0] ?? "");
      const problems = preflight(facts(next, args.includes("--allow-branch")));
      for (const p of problems) console.error(`✗ ${p}`);
      return problems.length ? 1 : 0;
    }
    case "bump": {
      const v = parseVersion(args[0] ?? "");
      const files = { tauriConf: readFileSync(FILES.tauriConf, "utf8"), cargoToml: readFileSync(FILES.cargoToml, "utf8"), packageJson: readFileSync(FILES.packageJson, "utf8") };
      const out = setVersion(files, v);
      for (const k of Object.keys(FILES) as (keyof typeof FILES)[]) writeFileSync(FILES[k], out[k]);
      return 0;
    }
    case "latest": {
      const [raw, sigPath, notesPath, outPath] = args;
      const version = parseVersion(raw ?? "");
      const json = buildLatestJson({
        version,
        notes: readFileSync(notesPath, "utf8").trim(),
        pubDate: new Date(),
        signature: readFileSync(sigPath, "utf8").trim(),
        url: assetUrl(version, ARCHIVE),
      });
      writeFileSync(outPath, `${JSON.stringify(json, null, 2)}\n`);
      return 0;
    }
    case "verify": {
      const [app, archive, sig] = args;
      const problems = verify(app, archive, sig);
      for (const p of problems) console.error(`✗ ${p}`);
      if (!problems.length) console.log(`✓ ${app}: «${IDENTITY}», ${BUNDLE_ID}, x86_64 + arm64, подпись обновления верна`);
      return problems.length ? 1 : 0;
    }
    default:
      console.error("команды: facts | bump | latest | verify");
      return 2;
  }
}

process.exitCode = main(process.argv.slice(2));
