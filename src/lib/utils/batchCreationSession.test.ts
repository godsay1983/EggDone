import { describe, it } from 'vitest';
import { createRequire } from 'node:module';
import * as api from './batchCreationSession';
const cases = createRequire(import.meta.url)('../../../scripts/batch-creation-cases.cjs')(api) as {name:string;run:()=>void|Promise<void>}[];
describe('batch creation session', () => { for (const test of cases) it(test.name, test.run); });
