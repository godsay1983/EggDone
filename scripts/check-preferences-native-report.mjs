// Validate the runner's evidence contract, not the native behavior independently.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';

export function checkNativePreferenceReport(result, phases) {
  assert.equal(result.schemaVersion, 1);
  assert.equal(result.passed, true, result.error || 'Native run did not pass');
  assert.equal(result.restartMode, 'forced-process', 'Do not mislabel this as a tray-exit test');
  assert.equal(result.error, null);
  assert.equal(result.restorationError, null);
  assert.equal(result.restoration, 'passed', 'Restoration is part of success');
  assert.equal(result.cleanupCompleted, true);
  assert.match(result.executable.replaceAll('\\', '/'), /\/src-tauri\/target\/search-native\/debug\/eggdone\.exe$/i);
  assert.match(result.sha256, /^[0-9a-f]{64}$/i);
  const writes = result.processes.filter(p => p.phase === 'write');
  const reads = result.processes.filter(p => p.phase === 'read');
  assert.equal(writes.length, 1);
  assert.equal(reads.length, 1);
  assert.ok(result.processes.every(p => ['write', 'read', 'restore'].includes(p.phase)));
  for (const process of result.processes) {
    assert.ok(Number.isInteger(process.pid) && process.pid > 0);
    assert.ok(Number.isFinite(Date.parse(process.started)));
  }
  assert.notEqual(writes[0].pid, reads[0].pid, 'Need two separately recorded native processes');
  assert.ok(Date.parse(reads[0].started) > Date.parse(writes[0].started));
  const required = {
    write: ['settings-ui-persisted'],
    read: ['restart-native-window-and-legacy-precedence', 'settings-controls-and-focus-window'],
    restore: ['isolated-preferences-restored'],
  };
  for (const [phase, checks] of Object.entries(required)) {
    const report = phases[phase];
    assert.ok(report, `Missing ${phase} report`);
    assert.equal(report.schemaVersion, 1);
    assert.equal(report.phase, phase);
    assert.equal(report.passed, true, report.error || `${phase} failed`);
    assert.equal(report.error, null);
    assert.deepEqual(report.pageErrors, []);
    assert.deepEqual(report.checks, checks);
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  const directory = process.argv[2];
  assert.ok(directory, 'Expected an evidence directory');
  const read = name => JSON.parse(readFileSync(resolve(directory, name), 'utf8').replace(/^\uFEFF/, ''));
  const result = read('result.json');
  // Report the primary failure even if startup did not create any phase report.
  assert.equal(result.passed, true, result.error || result.restorationError || 'Incomplete native run');
  checkNativePreferenceReport(result, Object.fromEntries(['write', 'read', 'restore'].map(phase => [phase, read(`${phase}-report.json`)])));
  console.log('PASS native preference evidence contract (forced process restart, not tray exit).');
}
