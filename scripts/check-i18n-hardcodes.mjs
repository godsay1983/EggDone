import { readFileSync, readdirSync, statSync } from "node:fs";
import { dirname, join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const sourceRoot = join(root, "src");
const allowlist = JSON.parse(
  readFileSync(join(root, "scripts/i18n-hardcode-allowlist.json"), "utf8"),
).entries;
const violations = [];

for (const entry of allowlist) {
  if (!entry.file || !entry.text || !entry.reason) {
    throw new Error("Every i18n hardcode allowlist entry requires file, text, and reason.");
  }
}

for (const file of walk(sourceRoot)) {
  if (!file.endsWith(".svelte")) continue;
  const source = readFileSync(file, "utf8");
  const template = source
    .replace(/<script\b[\s\S]*?<\/script>/gi, preserveLines)
    .replace(/<style\b[\s\S]*?<\/style>/gi, preserveLines);
  template.split(/\r?\n/).forEach((line, index) => {
    if (/[\u3400-\u9fff]/u.test(line)) {
      const relativePath = relative(root, file).replaceAll("\\", "/");
      const text = line.trim();
      const allowed = allowlist.some((entry) => entry.file === relativePath && text.includes(entry.text));
      if (!allowed) violations.push(`${relativePath}:${index + 1}: ${text}`);
    }
  });
}

if (violations.length > 0) {
  console.error("User-visible Chinese hardcode check failed. Move text to the i18n catalog or add a documented scanner exception.\n" +
    violations.map((item) => `- ${item}`).join("\n"));
  process.exit(1);
}
console.log("User-visible Chinese hardcode check passed for Svelte templates.");

function preserveLines(value) {
  return value.replace(/[^\r\n]/g, " ");
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
