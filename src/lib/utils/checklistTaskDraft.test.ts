import { createRequire } from 'node:module';
import { describe, it } from 'vitest';
import * as api from './checklistTaskDraft';
const require=createRequire(import.meta.url);
const cases: (module: typeof api) => {name:string;run:()=>Promise<void>}[] = require('../../../scripts/checklist-task-draft-cases.cjs');
describe('complete checklist task draft',()=>{for(const test of cases(api))it(test.name,test.run);});
