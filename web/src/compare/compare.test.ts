import cashflowSample from '../../../testdata/samples/cashflow_sample.csv?raw';
import { parseCsvText } from '../data/parseCsv';
import type { CashflowEvent } from '../data/types';
import { compareCashflows, compareReportCsv } from './compare';

const golden = parseCsvText(cashflowSample, 'cashflow').rows;
const clone = () => golden.map((e) => ({ ...e }));
const find = (rows: CashflowEvent[], deal: string, type: string, date: string) =>
  rows.find((e) => e.deal_id === deal && e.payment_type === type && e.payment_dttm === date)!;

describe('сверка с эталоном', () => {
  it('полное совпадение — все сделки зелёные', () => {
    const res = compareCashflows(clone(), golden);
    expect(res.rows.every((r) => r.status === 'match')).toBe(true);
    expect(res.deals.map((d) => d.ok)).toEqual([true, true, true]);
    expect(res.rows).toHaveLength(golden.length);
  });

  it('1 копейка: допуск 0.01 — совпало, 0.005 — расхождение', () => {
    const engine = clone();
    find(engine, 'DEMO-ANN-001', 'interest_payment', '2022-09-15').amount += 0.01;
    expect(compareCashflows(engine, golden, 0.01).deals.every((d) => d.ok)).toBe(true);
    const strict = compareCashflows(engine, golden, 0.005);
    expect(strict.rows.filter((r) => r.status === 'diff')).toHaveLength(1);
    expect(strict.deals.find((d) => d.dealId === 'DEMO-ANN-001')!.ok).toBe(false);
  });

  it('расхождение 0.02 при допуске 0.01', () => {
    const engine = clone();
    find(engine, 'DEMO-DEP-003', 'close_payment', '2022-12-31').principal_after = 0.02;
    const res = compareCashflows(engine, golden);
    const diff = res.rows.filter((r) => r.status === 'diff');
    expect(diff).toHaveLength(1);
    expect(diff[0]!.dPrincipal).toBeCloseTo(0.02, 10);
    const deal = res.deals.find((d) => d.dealId === 'DEMO-DEP-003')!;
    expect(deal).toMatchObject({ ok: false, diff: 1 });
    expect(deal.maxAbsDelta).toBeCloseTo(0.02, 10);
  });

  it('отсутствующая строка — «нет в движке»', () => {
    const engine = clone().filter(
      (e) =>
        !(
          e.deal_id === 'DEMO-FLT-002' &&
          e.payment_type === 'repricing' &&
          e.payment_dttm === '2022-10-01'
        ),
    );
    const res = compareCashflows(engine, golden);
    expect(res.rows.filter((r) => r.status === 'missing').map((r) => [r.dealId, r.type])).toEqual([
      ['DEMO-FLT-002', 'repricing'],
    ]);
  });

  it('лишняя строка — «лишнее в движке», дубли ключа по порядку', () => {
    const engine = clone();
    const extra = {
      ...find(engine, 'DEMO-ANN-001', 'interest_payment', '2022-10-15'),
      event_order: 99,
    };
    engine.push(extra);
    const res = compareCashflows(engine, golden);
    const bad = res.rows.filter((r) => r.status !== 'match');
    expect(bad).toHaveLength(1);
    expect(bad[0]).toMatchObject({ status: 'extra', dup: 1, dealId: 'DEMO-ANN-001' });
  });

  it('отчёт CSV', () => {
    const csv = compareReportCsv(compareCashflows(clone(), golden));
    const lines = csv.trim().split('\n');
    expect(lines[0]).toBe(
      'deal_id,payment_dttm,payment_type,stream,dup,status,golden_amount,engine_amount,delta_amount,golden_principal_after,engine_principal_after,delta_principal_after',
    );
    expect(lines).toHaveLength(golden.length + 1);
    expect(lines[1]).toContain(',match,');
  });
});
