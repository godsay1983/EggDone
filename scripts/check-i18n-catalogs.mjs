import { readFileSync, readdirSync, statSync } from "node:fs";
import { dirname, join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const zhPath = join(root, "src/lib/i18n/locales/zh-CN.ts");
const enPath = join(root, "src/lib/i18n/locales/en-US.ts");
const zhSource = readFileSync(zhPath, "utf8");
const enSource = readFileSync(enPath, "utf8");
const zhEntries = catalogEntries(zhSource);
const enEntries = catalogEntries(enSource);
const errors = [];

compareKeys(zhEntries, enEntries, "en-US");
compareKeys(enEntries, zhEntries, "zh-CN");
for (const key of zhEntries.keys()) {
  const zhPlaceholders = placeholders(zhEntries.get(key));
  const enPlaceholders = placeholders(enEntries.get(key));
  if (zhPlaceholders.join("|") !== enPlaceholders.join("|")) {
    errors.push(`${key}: placeholder mismatch zh=[${zhPlaceholders}] en=[${enPlaceholders}]`);
  }
}

for (const file of walk(join(root, "src"))) {
  if (!/\.(svelte|ts)$/.test(file) || file.includes(`${join("i18n", "locales")}`)) continue;
  const source = readFileSync(file, "utf8");
  const usagePattern = /(?:\$?translator)\(\s*["']([^"']+)["']/g;
  let match;
  while ((match = usagePattern.exec(source)) !== null) {
    if (!zhEntries.has(match[1])) {
      errors.push(`${relative(root, file)}: unknown translation key ${match[1]}`);
    }
  }
}

if (errors.length > 0) {
  console.error(`i18n catalog check failed:\n${errors.map((item) => `- ${item}`).join("\n")}`);
  process.exit(1);
}
console.log(`i18n catalog check passed: ${zhEntries.size} aligned keys with matching placeholders.`);

function catalogEntries(source) {
  const matches = [...source.matchAll(/^\s{2}"([^"]+)":/gm)];
  const entries = new Map();
  for (let index = 0; index < matches.length; index += 1) {
    const start = matches[index].index;
    const end = index + 1 < matches.length ? matches[index + 1].index : source.length;
    entries.set(matches[index][1], source.slice(start, end));
  }
  return entries;
}

function compareKeys(reference, candidate, locale) {
  for (const key of reference.keys()) {
    if (!candidate.has(key)) errors.push(`${locale}: missing key ${key}`);
  }
}

function placeholders(block = "") {
  return [...new Set([...block.matchAll(/\{([A-Za-z][A-Za-z0-9_]*)\}/g)].map((match) => match[1]))].sort();
}

function walk(directory) {
  const files = [];
  for (const name of readdirSync(directory)) {
    const path = join(directory, name);
    if (statSync(path).isDirectory()) files.push(...walk(path));
    else files.push(path);
  }
  return files;
}
