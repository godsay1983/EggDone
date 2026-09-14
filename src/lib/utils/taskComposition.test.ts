import { createRequire } from 'node:module';
import { readFileSync } from 'node:fs';
import { describe, it } from 'vitest';
import * as composition from './taskComposition';

interface ContractCase { name: string; run: () => void; }
type CaseFactory = (api: typeof composition, fixtures: unknown) => ContractCase[];
const require = createRequire(import.meta.url);
const createCases: CaseFactory = require('../../../scripts/task-composition-cases.cjs');
const fixtures: unknown = JSON.parse(readFileSync(new URL('../../../docs/fixtures/task-composition-v1.json', import.meta.url), 'utf8'));

describe('task composition shared contract v1', () => {
  for (const test of createCases(composition, fixtures)) it(test.name, test.run);
});
