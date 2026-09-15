import { describe, it } from 'vitest';
import { createRequire } from 'node:module';
import * as api from './taskCopyDraft';

const cases = createRequire(import.meta.url)('../../../scripts/task-copy-cases.cjs')(api) as {name:string;run:()=>void}[];
describe('task copy draft', () => { for (const test of cases) it(test.name, test.run); });
