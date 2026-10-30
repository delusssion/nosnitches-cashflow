import { parseCsvText } from '../data/parseCsv';
import cashflowSample from '../../../testdata/samples/cashflow_sample.csv?raw';
import { monthlyFlow } from './monthly';

describe('поток по месяцам', () => {
  const { rows } = parseCsvText(cashflowSample, 'cashflow');
  const buckets = monthlyFlow(rows);

  it('непрерывный ряд месяцев', () => {
    expect(buckets.map((b) => b.month)).toEqual([
      '2022-09',
      '2022-10',
      '2022-11',
      '2022-12',
      '2023-01',
      '2023-02',
    ]);
  });

  it('депозит уходит в пассивы, кредиты — в активы', () => {
    const dec = buckets.find((b) => b.month === '2022-12')!;
    expect(dec.liabilities.principal).toBe(-300000);
    expect(dec.liabilities.interest).toBeCloseTo(-1528.77, 2);
    expect(dec.assets.principal).toBeCloseTo(100524.72, 2);
  });

  it('repricing не попадает в поток', () => {
    const total = buckets.reduce((s, b) => s + b.assets.principal + b.liabilities.principal, 0);
    expect(total).toBeCloseTo(600000 + 500000 - 300000, 2);
  });
});
