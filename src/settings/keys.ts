import type { KeyId } from "../lib/types";

type T = (key: string, vars?: Record<string, string | number>) => string;

/** How a hotkey is written on a keycap. */
export function keyLabel(k: KeyId, t: T): string {
  if (typeof k === "object") return "F" in k ? `F${k.F}` : t("keys.raw", { code: k.Raw });
  const names: Record<string, string> = {
    RightAlt: "keys.right_alt",
    RightMeta: "keys.right_meta",
    RightCtrl: "keys.right_ctrl",
    RightShift: "keys.right_shift",
    CapsLock: "keys.caps_lock",
    Fn: "keys.fn",
    Escape: "keys.escape",
  };
  return t(names[k]);
}

export const sameKey = (a: KeyId, b: KeyId) => JSON.stringify(a) === JSON.stringify(b);
