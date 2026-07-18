import { describe, expect, it } from "vitest";

import { enUS } from "./locales/en-US";
import type { TranslationMessage } from "./types";

describe("pseudo localization", () => {
  it("expands every English message while preserving placeholders", () => {
    for (const [key, message] of Object.entries(enUS)) {
      for (const text of messageTexts(message)) {
        const pseudo = pseudoLocalize(text);
        expect(pseudo.length, key).toBeGreaterThan(text.length);
        expect(placeholders(pseudo), key).toEqual(placeholders(text));
      }
    }
  });
});

function pseudoLocalize(text: string): string {
  const tokens = text.split(/(\{[A-Za-z][A-Za-z0-9_]*\})/g);
  const expanded = tokens.map((token) => token.startsWith("{") ? token : `${accent(token)}${token}`).join("");
  return `[!! ${expanded} !!]`;
}

function accent(value: string): string {
  return value.replace(/[A-Za-z]/g, (letter) => ACCENTS[letter] ?? letter);
}

function messageTexts(message: TranslationMessage): string[] {
  return typeof message === "string" ? [message] : [message.one, message.other];
}

function placeholders(text: string): string[] {
  return [...text.matchAll(/\{([A-Za-z][A-Za-z0-9_]*)\}/g)].map((match) => match[1]).sort();
}

const ACCENTS: Record<string, string> = {
  a: "á", b: "ƀ", c: "ç", d: "đ", e: "ë", f: "ƒ", g: "ğ", h: "ħ", i: "ï", j: "ĵ",
  k: "ķ", l: "ł", m: "ɱ", n: "ñ", o: "ö", p: "þ", q: "ʠ", r: "ř", s: "š", t: "ŧ",
  u: "ü", v: "ṽ", w: "ŵ", x: "ẋ", y: "ÿ", z: "ž",
  A: "Á", B: "Ɓ", C: "Ç", D: "Đ", E: "Ë", F: "Ƒ", G: "Ğ", H: "Ħ", I: "Ï", J: "Ĵ",
  K: "Ķ", L: "Ł", M: "Ṁ", N: "Ñ", O: "Ö", P: "Þ", Q: "Q", R: "Ř", S: "Š", T: "Ŧ",
  U: "Ü", V: "Ṽ", W: "Ŵ", X: "Ẋ", Y: "Ÿ", Z: "Ž",
};
