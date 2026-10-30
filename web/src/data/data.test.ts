import cashflowSample from '../../../testdata/samples/cashflow_sample.csv?raw';
import dealsSample from '../../../testdata/samples/deals_sample.csv?raw';
import { parseCsvText } from './parseCsv';
import { buildStore, indexByDeal } from './store';
import type { CashflowEvent } from './types';

const HEADER =
  'report_dttm,deal_id,event_order,payment_dttm,payment_type,stream,amount,principal_before,principal_after,rate,base_rate,accrual_start_dttm,accrual_end_dttm,accrual_days,year_frac,discriminator';

const csv = (...lines: string[]) => [HEADER, ...lines].join('\n');

describe('разбор кэшфлоу', () => {
  it('корректная строка', () => {
    const res = parseCsvText(
      csv(
        '2022-08-31,D1,1,2022-09-15,interest_payment,client,6115.07,600000.00,600000.00,0.12,,2022-08-15,2022-09-15,31,0.0849315068,ANN',
      ),
      'cashflow',
    );
    expect(res.errors).toEqual([]);
    expect(res.rows).toHaveLength(1);
    expect(res.rows[0]).toMatchObject({
      deal_id: 'D1',
      event_order: 1,
      payment_dttm: '2022-09-15',
      payment_type: 'interest_payment',
      amount: 6115.07,
      rate: 0.12,
      base_rate: null,
      accrual_days: 31,
      discriminator: 'ANN',
    });
  });

  it('строка с ошибкой в дате', () => {
    const res = parseCsvText(
      csv(
        '2022-08-31,D1,1,2022-02-30,interest_payment,client,1,,,,,,,,,',
        '2022-08-31,D1,2,31.12.2022,close_payment,client,1,,,,,,,,,',
        '2022-08-31,D1,3,2022-12-31,close_payment,client,1,,,,,,,,,',
      ),
      'cashflow',
    );
    expect(res.rowCount).toBe(3);
    expect(res.rows).toHaveLength(1);
    expect(res.errors).toEqual([
      { row: 1, field: 'payment_dttm', message: 'ожидается дата YYYY-MM-DD' },
      { row: 2, field: 'payment_dttm', message: 'ожидается дата YYYY-MM-DD' },
    ]);
  });

  it('отрицательная сделка (депозит) — суммы со знаком', () => {
    const res = parseCsvText(
      csv(
        '2022-08-31,DEP,1,2022-09-30,interest_payment,client,-1479.45,-300000.00,-300000.00,0.06,,2022-08-31,2022-09-30,30,0.08,SIMPLE',
        '2022-08-31,DEP,2,2022-12-31,close_payment,client,-300000.00,-300000.00,0.00,0.06,,,,,,SIMPLE',
      ),
      'cashflow',
    );
    const [summary] = buildStore(res.rows).summaries;
    expect(summary).toMatchObject({
      interestTotal: -1479.45,
      principalTotal: -300000,
      finalBalance: 0,
      finalBalanceNonZero: false,
    });
  });

  it('пустые поля → null, пустое обязательное поле → ошибка', () => {
    const res = parseCsvText(
      csv(
        '2022-08-31,D1,1,2022-10-01,repricing,client,0,,,,,,,,,',
        '2022-08-31,,2,2022-10-01,repricing,client,,,,,,,,,,',
      ),
      'cashflow',
    );
    expect(res.rows[0]).toMatchObject({
      principal_before: null,
      rate: null,
      accrual_start_dttm: null,
      accrual_days: null,
      discriminator: null,
    });
    expect(res.errors.map((e) => [e.row, e.field])).toEqual([
      [2, 'deal_id'],
      [2, 'amount'],
    ]);
  });

  it('файл без нужных колонок — ошибка заголовка', () => {
    const res = parseCsvText('deal_id,amount\nD1,1', 'cashflow');
    expect(res.rows).toHaveLength(0);
    expect(res.errors[0]).toMatchObject({ row: 0, field: 'report_dttm' });
  });
});

describe('стор', () => {
  const ev = (p: Partial<CashflowEvent>): CashflowEvent => ({
    report_dttm: '2022-08-31',
    deal_id: 'D',
    event_order: 1,
    payment_dttm: '2022-09-01',
    payment_type: 'interest_payment',
    stream: 'client',
    amount: 0,
    principal_before: null,
    principal_after: null,
    rate: null,
    base_rate: null,
    accrual_start_dttm: null,
    accrual_end_dttm: null,
    accrual_days: null,
    year_frac: null,
    discriminator: null,
    ...p,
  });

  it('порядок событий: по дате, затем event_order', () => {
    const index = indexByDeal([
      ev({ event_order: 3, payment_dttm: '2022-11-01', payment_type: 'close_payment' }),
      ev({ event_order: 2, payment_dttm: '2022-10-01', payment_type: 'interest_payment' }),
      ev({ event_order: 1, payment_dttm: '2022-10-01', payment_type: 'repricing' }),
      ev({ deal_id: 'X' }),
    ]);
    expect(index.get('D')!.map((e) => e.event_order)).toEqual([1, 2, 3]);
    expect([...index.keys()]).toEqual(['D', 'X']);
  });

  it('сводка: субсидия отдельно, флаг ненулевого остатка', () => {
    const { summaries } = buildStore([
      ev({ amount: 100 }),
      ev({ event_order: 2, payment_type: 'sub_payment', stream: 'sub', amount: 10 }),
      ev({ event_order: 3, payment_type: 'principal_payment', amount: 500, principal_after: 0.01 }),
    ]);
    expect(summaries[0]).toMatchObject({
      eventCount: 3,
      interestTotal: 100,
      subsidyTotal: 10,
      principalTotal: 500,
      lastDate: '2022-09-01',
      finalBalanceNonZero: true,
    });
  });
});

describe('демо-данные из testdata/samples', () => {
  it('кэшфлоу валиден, остатки нулевые', () => {
    const res = parseCsvText(cashflowSample, 'cashflow');
    expect(res.errors).toEqual([]);
    const { summaries } = buildStore(res.rows);
    expect(summaries).toHaveLength(3);
    expect(summaries.every((s) => !s.finalBalanceNonZero)).toBe(true);
  });

  it('сделки валидны', () => {
    const res = parseCsvText(dealsSample, 'deals');
    expect(res.errors).toEqual([]);
    expect(res.rows.map((d) => d.discriminator)).toEqual(['ANN', 'SIMPLE', 'SIMPLE']);
  });
});

describe('быстрый путь и zod согласованы', () => {
  it('на демо-данных дают одинаковый результат', async () => {
    const Papa = (await import('papaparse')).default;
    const { cashflowSchema } = await import('./schemas');
    const raw = Papa.parse<Record<string, string>>(cashflowSample, {
      header: true,
      skipEmptyLines: 'greedy',
    }).data;
    const fast = parseCsvText(cashflowSample, 'cashflow').rows;
    expect(fast).toEqual(raw.map((r) => cashflowSchema.parse(r)));
  });
});
