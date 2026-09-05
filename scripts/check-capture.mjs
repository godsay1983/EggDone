import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import vm from "node:vm";
import ts from "typescript";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const peer = process.argv.find((arg) => arg.startsWith("--harmony="))?.slice(10);
const fixtureText = readFileSync(resolve(root, "docs/fixtures/capture-v1.json"), "utf8");
const fixtures = JSON.parse(fixtureText);
const paths = [resolve(root, "src/lib/utils/capture.ts")];
if (peer) {
  assert.equal(readFileSync(resolve(peer, "docs/fixtures/capture-v1.json"), "utf8"), fixtureText);
  paths.push(resolve(peer, "EggDone/entry/src/main/ets/services/capture/CaptureIntentParser.ets"));
}
for (const path of paths) {
  const exports = {};
  const code = ts.transpileModule(readFileSync(path, "utf8"), {
    compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS },
  }).outputText;
  vm.runInNewContext(code, { exports });
  const defaults = { target: "note", title: "", body: "", source_url: "", source_app: "" };
  for (const test of fixtures) {
    if (test.error) {
      assert.throws(() => exports.normalizeCapture({ ...defaults, ...test.input }), test.id);
    } else {
      const draft = exports.normalizeCapture({ ...defaults, ...test.input });
      assert.equal(draft.title, test.title, test.id);
      assert.equal(draft.body, test.body, test.id);
      assert.equal(draft.truncated, test.truncated, test.id);
      assert.equal(exports.captureTitle(draft), test.fallback, test.id);
      if (test.content) assert.equal(exports.captureContent(draft), test.content, test.id);
    }
  }
  const long = exports.normalizeCapture({ ...defaults, title: "x".repeat(99) + "😀", body: "a".repeat(20001) });
  assert.equal(long.title.length, 99);
  assert.equal(long.body.length, 20000);
  assert.equal(long.truncated, true);
  assert.throws(() => exports.normalizeCapture({ ...defaults, body: "a".repeat(100001) }));
  console.log(path + ": " + (fixtures.length + 2) + " capture fixtures passed");
}
