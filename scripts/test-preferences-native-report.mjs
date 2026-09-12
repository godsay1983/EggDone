import assert from 'node:assert/strict';
import { test } from 'node:test';
import { checkNativePreferenceReport } from './check-preferences-native-report.mjs';

function fixture() {
  return {
    result: { schemaVersion: 1, passed: true, restartMode: 'forced-process', error: null,
      restorationError: null, restoration: 'passed', cleanupCompleted: true,
      executable: 'D:\\Develop\\EggDone\\src-tauri\\target\\search-native\\debug\\eggdone.exe',
      sha256: 'a'.repeat(64), processes: [
        { phase: 'write', pid: 101, started: '2026-09-13T01:00:00Z' },
        { phase: 'read', pid: 102, started: '2026-09-13T01:01:00Z' },
      ] },
    phases: Object.fromEntries(Object.entries({
      write: ['settings-ui-persisted'],
      read: ['restart-native-window-and-legacy-precedence', 'settings-controls-and-focus-window'],
      restore: ['isolated-preferences-restored'],
    }).map(([phase, checks]) => [phase, { schemaVersion: 1, phase, passed: true, checks, pageErrors: [], error: null }])),
  };
}

test('complete synthetic evidence satisfies the contract, not native acceptance', () => {
  const { result, phases } = fixture();
  checkNativePreferenceReport(result, phases);
});

const mutations = {
  'startup timeout': f => { f.result.passed = false; f.result.error = 'startup timeout'; },
  'wrong evidence version': f => { f.result.schemaVersion = 2; },
  'not a tray-exit proof': f => { f.result.restartMode = 'tray-exit'; },
  'ordinary executable': f => { f.result.executable = 'D:/Develop/EggDone/src-tauri/target/debug/eggdone.exe'; },
  'missing binary hash': f => { f.result.sha256 = ''; },
  'same process reused': f => { f.result.processes[1].pid = 101; },
  'missing restart': f => { f.result.processes.pop(); },
  'duplicate read': f => { f.result.processes.push({ ...f.result.processes[1] }); },
  'bad process time': f => { f.result.processes[1].started = 'invalid'; },
  'reversed process time': f => { f.result.processes[1].started = '2026-09-12T01:00:00Z'; },
  'restore not run': f => { f.result.restoration = 'not-needed'; },
  'restore error': f => { f.result.restorationError = 'restore failed'; },
  'cleanup incomplete': f => { f.result.cleanupCompleted = false; },
  'missing phase': f => { delete f.phases.read; },
  'phase did not pass': f => { f.phases.read.passed = false; },
  'wrong phase identity': f => { f.phases.read.phase = 'write'; },
  'missing focus check': f => { f.phases.read.checks.pop(); },
  'page error': f => { f.phases.read.pageErrors.push('native failure'); },
  'phase error hidden by flag': f => { f.phases.write.error = 'write failed'; },
};
for (const [name, mutate] of Object.entries(mutations)) {
  test(`reject ${name}`, () => {
    const f = fixture(); mutate(f);
    assert.throws(() => checkNativePreferenceReport(f.result, f.phases));
  });
}
