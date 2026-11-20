import engineCashflow from '../../../testdata/samples/engine/cashflow.csv?raw';
import engineErrors from '../../../testdata/samples/engine/errors.csv?raw';
import dealsSample from '../../../testdata/samples/deals_sample.csv?raw';
import { parseCsvText } from './parseCsv';
import { buildStore, indexByDeal } from './store';
import type { CashflowEvent } from './types';

describe('вывод движка cfgen (спринт 3)', () => {
  it('кэшфлоу 55 тестовых сделок валиден, остатки нулевые', () => {
    const res = parseCsvText(engineCashflow, 'cashflow');
    expect(res.errors).toEqual([]);
    const { summaries } = buildStore(res.rows);
    expect(summaries).toHaveLength(54); // сделка 32 (USD) отклонена
    expect(summaries.filter((s) => s.finalBalanceNonZero)).toEqual([]);
  });

  it('errors.csv разбирается: USD — ошибка, остальное — предупреждения', () => {
    const res = parseCsvText(engineErrors, 'errors');
    expect(res.errors).toEqual([]);
    const errors = res.rows.filter((r) => r.severity === 'error');
    expect(errors.map((r) => [r.deal_id, r.code])).toEqual([['32', 'UNSUPPORTED_CURRENCY']]);
    expect(res.rows.some((r) => r.code === 'SCHEDULE_SUM_MISMATCH' && r.deal_id === '20')).toBe(
      true,
    );
  });

  it('колонка prepayment_schedule необязательна', () => {
    const [header, ...lines] = dealsSample.trim().split('\n');
    const withCol = [
      header + ',prepayment_schedule',
      ...lines.map((l, i) => l + (i === 0 ? ',"2022-10-15,50000"' : ',')),
    ].join('\n');
    const res = parseCsvText(withCol, 'deals');
    expect(res.errors).toEqual([]);
    expect(res.rows[0]!.prepayment_schedule).toBe('2022-10-15,50000');
    expect(res.rows[1]!.prepayment_schedule).toBeNull();
    // и без колонки файл по-прежнему читается
    expect(parseCsvText(dealsSample, 'deals').errors).toEqual([]);
  });

  it('досрочное погашение идёт перед плановым телом того же дня, входит в сумму тела', () => {
    const base = {
      report_dttm: '2022-08-31',
      deal_id: 'P',
      stream: 'client',
      principal_before: null,
      rate: null,
      base_rate: null,
      accrual_start_dttm: null,
      accrual_end_dttm: null,
      accrual_days: null,
      year_frac: null,
      discriminator: null,
    } as const;
    const ev = (p: Partial<CashflowEvent>): CashflowEvent =>
      ({
        ...base,
        event_order: 1,
        payment_dttm: '2022-10-01',
        payment_type: 'interest_payment',
        amount: 0,
        principal_after: null,
        ...p,
      }) as CashflowEvent;
    const events = [
      ev({ payment_type: 'principal_payment', amount: 100, principal_after: 50 }),
      ev({ payment_type: 'prepayment', amount: 250, principal_after: 150 }),
      ev({ payment_type: 'interest_payment', amount: 5 }),
    ];
    const list = indexByDeal(events).get('P')!;
    expect(list.map((e) => e.payment_type)).toEqual([
      'interest_payment',
      'prepayment',
      'principal_payment',
    ]);
    const [s] = buildStore(events).summaries;
    expect(s).toMatchObject({ principalTotal: 350, prepaymentTotal: 250 });
  });
});
