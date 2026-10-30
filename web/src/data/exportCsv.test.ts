import cashflowSample from '../../../testdata/samples/cashflow_sample.csv?raw';
import { cashflowToCsv } from './exportCsv';
import { parseCsvText } from './parseCsv';

it('экспорт CSV сделки совпадает с исходным форматом', () => {
  const { rows } = parseCsvText(cashflowSample, 'cashflow');
  const dep = rows.filter((r) => r.deal_id === 'DEMO-DEP-003');
  const csv = cashflowToCsv(dep);
  const expected = cashflowSample
    .split('\n')
    .filter((l, i) => i === 0 || l.includes('DEMO-DEP-003'))
    .join('\n');
  expect(csv).toBe(expected + '\n');
  expect(parseCsvText(csv, 'cashflow').rows).toEqual(dep);
});
