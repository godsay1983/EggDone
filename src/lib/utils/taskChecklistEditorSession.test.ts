import { createRequire } from 'node:module';
import { describe, it } from 'vitest';
import { TaskChecklistEditorSession } from './taskChecklistEditorSession';

interface ContractCase { name: string; run: () => Promise<void>; }
type CaseFactory = (session: typeof TaskChecklistEditorSession) => ContractCase[];
const require = createRequire(import.meta.url);
const cases: CaseFactory = require('../../../scripts/task-checklist-editor-session-cases.cjs');
describe('full checklist editor session', () => {
  for (const test of cases(TaskChecklistEditorSession)) it(test.name, test.run);
});
