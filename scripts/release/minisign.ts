// Checks an update archive's signature the way the installed Utter will (Tauri updater = minisign).
// Tauri stores the public key and the .sig as base64 of minisign's text files.
import { createHash, createPublicKey, verify } from "node:crypto";

const ED25519_SPKI = Buffer.from("302a300506032b6570032100", "hex");

function lines(base64OrText: string): string[] {
  const text = base64OrText.includes("untrusted comment") ? base64OrText : Buffer.from(base64OrText.trim(), "base64").toString("utf8");
  return text.split("\n").map((l) => l.trim());
}

function ed25519(key: Buffer, msg: Buffer, sig: Buffer): boolean {
  try {
    return verify(null, msg, createPublicKey({ key: Buffer.concat([ED25519_SPKI, key]), format: "der", type: "spki" }), sig);
  } catch {
    return false;
  }
}

export function verifyMinisign(file: Buffer, sigFileText: string, pubKeyText: string): boolean {
  try {
    const pk = Buffer.from(lines(pubKeyText)[1], "base64");
    const s = lines(sigFileText);
    const sig = Buffer.from(s[1], "base64");
    if (pk.length !== 42 || sig.length !== 74) return false;
    const alg = sig.subarray(0, 2).toString("latin1");
    if (!sig.subarray(2, 10).equals(pk.subarray(2, 10))) return false; // signed by another key
    const key = pk.subarray(10);
    const msg = alg === "ED" ? createHash("blake2b512").update(file).digest() : alg === "Ed" ? file : null;
    if (!msg || !ed25519(key, msg, sig.subarray(10))) return false;
    // The trusted comment is signed too: sig || comment text.
    const comment = s[2]?.replace(/^trusted comment: /, "");
    const global = s[3] ? Buffer.from(s[3], "base64") : Buffer.alloc(0);
    return comment !== undefined && ed25519(key, Buffer.concat([sig.subarray(10), Buffer.from(comment, "utf8")]), global);
  } catch {
    return false;
  }
}
