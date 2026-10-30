// Демо-данные для UI (НЕ эталон): testdata/samples/deals_sample.csv и cashflow_sample.csv.
// Упрощённый расчёт по формулам docs/CONTRACT.md. Запуск: npm run make-sample
import { writeFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const OUT_DIR = resolve(dirname(fileURLToPath(import.meta.url)), '../../testdata/samples');
const REPORT = '2022-08-31';

const DEAL_COLUMNS = [
  'report_dttm',
  'discriminator',
  'deal_id',
  'currency_id',
  'start_principal_amt',
  'current_principal_amt',
  'deal_dttm',
  'maturity_dttm',
  'rate_type_cd',
  'interest_rate',
  'base_indicator_rate',
  'base_indicator',
  'interest_payment_rule_start_dttm',
  'interest_payment_rule_period_cd',
  'interest_payment_rule_day_num',
  'day_convention_cd',
  'regular_payment_rule_start_dttm',
  'regular_payment_rule_period_cd',
  'regular_payment_rule_day_num',
  'repricing_rule_start_dttm',
  'repricing_rule_period_cd',
  'repricing_rule_day_num',
  'accrued_interest',
  'product',
  'behavior_model_cd',
  'interest_payment',
  'principal_payment',
  'repricing_dttm_schedule',
  'repricing_dttm_rate_schedule',
  'repricing_dttm_rate_schedule_fixed',
  'cap_limit_interest_rate',
  'floor_limit_interest_rate',
  'sub_rate_type_cd',
  'sub_interest_rate',
  'sub_float_indicator',
  'sub_payment_rule_period_cd',
  'sub_payment_rule_day_num',
  'sub_payment_rule_start_dttm',
  'sub_repricing_rule_start_dttm',
  'sub_repricing_rule_period_cd',
  'sub_repricing_rule_day_num',
  'sub_cap_limit_interest_rate',
  'sub_floor_limit_interest_rate',
  'sub_coeff_index',
  'deal_report_dttm',
  'sub_repricing_rule_first_event_flg',
  'indicator_lag_cd',
  'sub_indicator_lag_cd',
] as const;

const CF_COLUMNS = [
  'report_dttm',
  'deal_id',
  'event_order',
  'payment_dttm',
  'payment_type',
  'stream',
  'amount',
  'principal_before',
  'principal_after',
  'rate',
  'base_rate',
  'accrual_start_dttm',
  'accrual_end_dttm',
  'accrual_days',
  'year_frac',
  'discriminator',
] as const;

type DealRow = Partial<Record<(typeof DEAL_COLUMNS)[number], string | number>>;
type CfRow = Partial<Record<(typeof CF_COLUMNS)[number], string | number>>;

const DAY_MS = 86_400_000;
const days = (a: string, b: string) => Math.round((Date.parse(b) - Date.parse(a)) / DAY_MS);

// Даты платежей: каждый месяц числа dayNum (-1 = последний день месяца) в интервале (from, to].
function monthlyDates(from: string, to: string, dayNum: number): string[] {
  const out: string[] = [];
  let [y, m] = from.split('-').map(Number) as [number, number];
  for (;;) {
    const last = new Date(Date.UTC(y, m, 0)).getUTCDate();
    const d = dayNum === -1 ? last : Math.min(dayNum, last);
    const iso = `${y}-${String(m).padStart(2, '0')}-${String(d).padStart(2, '0')}`;
    if (iso > to) break;
    if (iso > from) out.push(iso);
    m += 1;
    if (m > 12) [y, m] = [y + 1, 1];
  }
  return out;
}

const r2 = (x: number) => (Math.round(x * 100) / 100).toFixed(2);

interface Sim {
  deal: DealRow;
  events: CfRow[];
}

function interestEvent(
  dealId: string,
  disc: string,
  date: string,
  start: string,
  principal: number,
  rate: number,
  baseRate: number | null,
): CfRow {
  const d = days(start, date);
  const yf = d / 365; // ACT/365
  return {
    deal_id: dealId,
    payment_dttm: date,
    payment_type: 'interest_payment',
    stream: 'client',
    amount: principal * rate * yf,
    principal_before: principal,
    principal_after: principal,
    rate,
    base_rate: baseRate ?? '',
    accrual_start_dttm: start,
    accrual_end_dttm: date,
    accrual_days: d,
    year_frac: yf,
    discriminator: disc,
  };
}

function bodyEvent(
  dealId: string,
  disc: string,
  date: string,
  type: 'principal_payment' | 'close_payment',
  amount: number,
  before: number,
  rate: number,
  baseRate: number | null,
): CfRow {
  return {
    deal_id: dealId,
    payment_dttm: date,
    payment_type: type,
    stream: 'client',
    amount,
    principal_before: before,
    principal_after: type === 'close_payment' ? 0 : before - amount,
    rate,
    base_rate: baseRate ?? '',
    discriminator: disc,
  };
}

// 1) ANN 600 000, 12% FIXED, 15-го числа, до 2023-02-15. Аннуитет от остатка на дату расчёта (H7).
function annDeal(): Sim {
  const id = 'DEMO-ANN-001';
  const P = 600_000;
  const rate = 0.12;
  const start = '2022-08-15';
  const maturity = '2023-02-15';
  const dates = monthlyDates(start, maturity, 15);
  const n = dates.length;
  const r = rate / 12; // H7: ставка периода = годовая × месяцы / 12
  const annuity = (P * r) / (1 - (1 + r) ** -n);
  const events: CfRow[] = [];
  let bal = P;
  let prev = start;
  dates.forEach((date, i) => {
    const interest = interestEvent(id, 'ANN', date, prev, bal, rate, null);
    events.push(interest);
    const isLast = i === n - 1;
    const body = isLast ? bal : annuity - Number(interest.amount);
    events.push(
      bodyEvent(
        id,
        'ANN',
        date,
        isLast ? 'close_payment' : 'principal_payment',
        body,
        bal,
        rate,
        null,
      ),
    );
    bal -= body;
    prev = date;
  });
  return {
    deal: {
      discriminator: 'ANN',
      deal_id: id,
      start_principal_amt: P,
      current_principal_amt: P,
      deal_dttm: start,
      maturity_dttm: maturity,
      rate_type_cd: 'FIXED',
      interest_rate: rate,
      interest_payment_rule_start_dttm: start,
      interest_payment_rule_period_cd: '1M',
      interest_payment_rule_day_num: 15,
      day_convention_cd: 'ACT/365',
      regular_payment_rule_start_dttm: start,
      regular_payment_rule_period_cd: '1M',
      regular_payment_rule_day_num: 15,
      product: 'DEMO_LOAN_ANN',
    },
    events,
  };
}

// 2) SIMPLE FLOAT CBKEY1M + 0.03, 500 000, тело в конце 2023-01-01, проценты 1-го, репрайсинг 10-01 и 12-01.
// Индикатор для демо: 8% до 2022-10-01, 7.5% после (H2: ставка = индикатор + маржа).
function floatDeal(): Sim {
  const id = 'DEMO-FLT-002';
  const P = 500_000;
  const margin = 0.03;
  const start = '2022-08-01';
  const maturity = '2023-01-01';
  const repricing = ['2022-10-01', '2022-12-01'];
  const indicator = (date: string) => (date < '2022-10-01' ? 0.08 : 0.075);
  const events: CfRow[] = [];
  let base = indicator(start);
  let prev = start;
  for (const date of monthlyDates(start, maturity, 1)) {
    events.push(interestEvent(id, 'SIMPLE', date, prev, P, base + margin, base));
    if (repricing.includes(date)) {
      base = indicator(date);
      events.push({
        deal_id: id,
        payment_dttm: date,
        payment_type: 'repricing',
        stream: 'client',
        amount: 0,
        principal_before: P,
        principal_after: P,
        rate: base + margin,
        base_rate: base,
        discriminator: 'SIMPLE',
      });
    }
    prev = date;
  }
  events.push(bodyEvent(id, 'SIMPLE', maturity, 'close_payment', P, P, base + margin, base));
  return {
    deal: {
      discriminator: 'SIMPLE',
      deal_id: id,
      start_principal_amt: P,
      current_principal_amt: P,
      deal_dttm: start,
      maturity_dttm: maturity,
      rate_type_cd: 'FLOAT',
      interest_rate: margin,
      base_indicator_rate: 0.08,
      base_indicator: 'CBKEY1M',
      interest_payment_rule_start_dttm: start,
      interest_payment_rule_period_cd: '1M',
      interest_payment_rule_day_num: 1,
      day_convention_cd: 'ACT/365',
      regular_payment_rule_period_cd: 'ENDDATE',
      product: 'DEMO_LOAN_FLOAT',
      repricing_dttm_schedule: repricing.join(' '),
      indicator_lag_cd: '-1M',
    },
    events,
  };
}

// 3) SIMPLE депозит -300 000, 6% FIXED, проценты в последний день месяца, тело в конце 2022-12-31.
function depositDeal(): Sim {
  const id = 'DEMO-DEP-003';
  const P = -300_000;
  const rate = 0.06;
  const start = '2022-06-30';
  const maturity = '2022-12-31';
  const events: CfRow[] = [];
  let prev = start;
  for (const date of monthlyDates(start, maturity, -1)) {
    events.push(interestEvent(id, 'SIMPLE', date, prev, P, rate, null));
    prev = date;
  }
  events.push(bodyEvent(id, 'SIMPLE', maturity, 'close_payment', P, P, rate, null));
  return {
    deal: {
      discriminator: 'SIMPLE',
      deal_id: id,
      start_principal_amt: P,
      current_principal_amt: P,
      deal_dttm: start,
      maturity_dttm: maturity,
      rate_type_cd: 'FIXED',
      interest_rate: rate,
      interest_payment_rule_start_dttm: start,
      interest_payment_rule_period_cd: '1M',
      interest_payment_rule_day_num: -1,
      day_convention_cd: 'ACT/365',
      regular_payment_rule_period_cd: 'ENDDATE',
      product: 'DEMO_DEPOSIT',
    },
    events,
  };
}

const ORDER: Record<string, number> = {
  repricing: 0,
  sub_repricing: 0,
  interest_payment: 1,
  sub_payment: 2,
  principal_payment: 3,
  close_payment: 4,
};

function finalize(sim: Sim): CfRow[] {
  const future = sim.events
    .filter((e) => String(e.payment_dttm) > REPORT) // H6: событие в дату расчёта — прошлое
    .sort(
      (a, b) =>
        String(a.payment_dttm).localeCompare(String(b.payment_dttm)) ||
        ORDER[String(a.payment_type)]! - ORDER[String(b.payment_type)]!,
    );
  // Проценты, накопленные до даты расчёта, входят в первый будущий interest_payment — уже так,
  // потому что период начисления первого платежа начинается до REPORT.
  const bodyTotal = future
    .filter((e) => e.payment_type === 'principal_payment' || e.payment_type === 'close_payment')
    .reduce((s, e) => s + Number(e.amount), 0);
  const current = Number(sim.deal.current_principal_amt);
  const last = future.at(-1)!;
  if (Math.abs(bodyTotal - current) > 0.005 || Number(last.principal_after) !== 0) {
    throw new Error(`${sim.deal.deal_id}: тело ${bodyTotal} ≠ ${current} или остаток ≠ 0`);
  }
  return future.map((e, i) => ({
    ...e,
    report_dttm: REPORT,
    event_order: i + 1,
    amount: r2(Number(e.amount)),
    principal_before: r2(Number(e.principal_before)),
    principal_after: r2(Number(e.principal_after)),
    year_frac: e.year_frac === undefined ? '' : Number(e.year_frac).toFixed(10),
  }));
}

function toCsv<C extends readonly string[]>(
  columns: C,
  rows: Partial<Record<C[number], string | number>>[],
) {
  const cell = (v: string | number | undefined) => {
    const s = v === undefined ? '' : String(v);
    return /[",\n]/.test(s) ? `"${s.replaceAll('"', '""')}"` : s;
  };
  const lines = rows.map((row) => columns.map((c) => cell(row[c as C[number]])).join(','));
  return [columns.join(','), ...lines].join('\n') + '\n';
}

const sims = [annDeal(), floatDeal(), depositDeal()];
const deals = sims.map((s) => ({
  report_dttm: REPORT,
  currency_id: 'RUB',
  deal_report_dttm: REPORT,
  ...s.deal,
}));
writeFileSync(resolve(OUT_DIR, 'deals_sample.csv'), toCsv(DEAL_COLUMNS, deals));
writeFileSync(resolve(OUT_DIR, 'cashflow_sample.csv'), toCsv(CF_COLUMNS, sims.flatMap(finalize)));
console.log(`OK: ${sims.length} сделки → ${OUT_DIR}`);
