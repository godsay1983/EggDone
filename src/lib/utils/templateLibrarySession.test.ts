import { describe, it } from 'vitest';
import { createRequire } from 'node:module';
import * as api from './templateLibrarySession';
const cases = createRequire(import.meta.url)('../../../scripts/template-library-cases.cjs')(api) as {name:string;run:()=>void|Promise<void>}[];
describe('template library', () => { for (const test of cases) it(test.name, test.run); });
