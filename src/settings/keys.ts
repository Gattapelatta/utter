import type { KeyId } from "../lib/types";

type T = (key: string, vars?: Record<string, string | number>) => string;

/** What is printed on a Mac key, by its key code (US layout; any key can be a hotkey, 2026-11-10). */
const MAC_KEYS: Record<number, string> = {
  0: "A", 1: "S", 2: "D", 3: "F", 4: "H", 5: "G", 6: "Z", 7: "X", 8: "C", 9: "V", 10: "§", 11: "B",
  12: "Q", 13: "W", 14: "E", 15: "R", 16: "Y", 17: "T", 18: "1", 19: "2", 20: "3", 21: "4", 22: "6",
  23: "5", 24: "=", 25: "9", 26: "7", 27: "-", 28: "8", 29: "0", 30: "]", 31: "O", 32: "U", 33: "[",
  34: "I", 35: "P", 36: "↩", 37: "L", 38: "J", 39: "'", 40: "K", 41: ";", 42: "\\", 43: ",", 44: "/",
  45: "N", 46: "M", 47: ".", 48: "⇥", 50: "`", 51: "⌫", 65: "Num .", 67: "Num *", 69: "Num +",
  75: "Num /", 76: "Num ↩", 78: "Num -", 81: "Num =", 82: "Num 0", 83: "Num 1", 84: "Num 2",
  85: "Num 3", 86: "Num 4", 87: "Num 5", 88: "Num 6", 89: "Num 7", 91: "Num 8", 92: "Num 9",
  115: "Home", 116: "Page Up", 117: "⌦", 119: "End", 121: "Page Down", 123: "←", 124: "→", 125: "↓", 126: "↑",
};

/** How a hotkey is written on a keycap. */
export function keyLabel(k: KeyId, t: T): string {
  if (typeof k === "object") {
    if ("F" in k) return `F${k.F}`;
    if (k.Raw === 49) return t("keys.space");
    return MAC_KEYS[k.Raw] ?? t("keys.raw", { code: k.Raw });
  }
  const names: Record<string, string> = {
    RightAlt: "keys.right_alt",
    RightMeta: "keys.right_meta",
    RightCtrl: "keys.right_ctrl",
    RightShift: "keys.right_shift",
    LeftAlt: "keys.left_alt",
    LeftMeta: "keys.left_meta",
    LeftCtrl: "keys.left_ctrl",
    LeftShift: "keys.left_shift",
    CapsLock: "keys.caps_lock",
    Fn: "keys.fn",
    Escape: "keys.escape",
  };
  return t(names[k]);
}

/** A key that types text: as a hotkey it stops typing (Utter swallows it). */
export const typesText = (k: KeyId | null | undefined) => !!k && typeof k === "object" && "Raw" in k;

export const sameKey = (a: KeyId, b: KeyId) => JSON.stringify(a) === JSON.stringify(b);
