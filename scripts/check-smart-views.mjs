import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";
import vm from "node:vm";
import ts from "typescript";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const fixturePath = resolve(root, "docs/fixtures/smart-views-v1.json");
const fixture = JSON.parse(readFileSync(fixturePath, "utf8"));
const peer = process.argv.find((arg) => arg.startsWith("--harmony="))?.slice(10);
if (!process.argv.includes("--worker")) {
  if (peer) assert.equal(readFileSync(resolve(peer, "docs/fixtures/smart-views-v1.json"), "utf8"),
    readFileSync(fixturePath, "utf8"), "Shared fixture copies differ");
  for (const zone of fixture.zones) {
    const run = spawnSync(process.execPath, [fileURLToPath(import.meta.url), "--worker",
      ...(peer ? ["--harmony=" + peer] : [])], { env: { ...process.env, TZ: zone }, encoding: "utf8" });
    process.stdout.write(run.stdout);
    process.stderr.write(run.stderr);
    assert.equal(run.status, 0, zone + " fixture worker failed");
  }
} else {
  // Execute the real pure TS/ArkTS implementations, not test-side copies.
  const compile = (path) => {
    const source = ts.transpileModule(readFileSync(path, "utf8"), {
      compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS },
    }).outputText;
    const module = { exports: {} };
    vm.runInNewContext(source, { exports: module.exports, module, Date, Number });
    return module.exports;
  };
  const desktop = compile(resolve(root, "src/lib/utils/smartViews.ts"));
  const harmony = peer ? compile(resolve(peer,
    "EggDone/entry/src/main/ets/services/views/SmartViewService.ets")) : null;
  let count = 0;
  for (const row of fixture.cases) {
    if (row.zone && row.zone !== process.env.TZ) continue;
    const task = { completed: false, completedAt: null, deletedAt: null,
      archivedAt: null, dueAt: null, dueDate: null, priority: 0, ...row.task };
    for (const key of ["completedAt", "deletedAt", "archivedAt", "dueAt"]) {
      if (typeof task[key] === "string") task[key] = new Date(task[key]).getTime();
    }
    const now = new Date(row.now).getTime();
    const expected = row.expected.join(",");
    const result = desktop.SMART_VIEW_IDS.filter((id) => desktop.matchesSmartView(task, id, now));
    assert.equal(result.join(","), expected, row.id + " desktop");
    if (harmony) {
      const actual = desktop.SMART_VIEW_IDS.filter((id) => harmony.matchesSmartView(task, id, now));
      assert.equal(actual.join(","), expected, row.id + " harmony");
      // Harmony stores date-only values as local midnight; test the production DTO adapter too.
      if (row.id !== "invalid-date") {
        const todo = { ...task, dueAt: task.dueDate ? new Date(task.dueDate + "T00:00:00").getTime() : task.dueAt };
        assert.equal(desktop.SMART_VIEW_IDS.filter((id) =>
          harmony.matchesTodoSmartView(todo, id, now)).join(","), expected, row.id + " DTO");
      }
    }
    count++;
  }
  console.log(process.env.TZ + ": " + count + " fixtures passed" + (peer ? " in both engines and Harmony DTO" : ""));
}
