import { createRequire } from 'node:module';
import { describe, it } from 'vitest';
import * as scope from './checklistScopeDraft';
interface Case { name: string; run: () => void; }
const require=createRequire(import.meta.url);
const cases: (api: typeof scope) => Case[] = require('../../../scripts/checklist-scope-cases.cjs');
describe('checklist scope draft',()=>{for(const test of cases(scope))it(test.name,test.run);});
