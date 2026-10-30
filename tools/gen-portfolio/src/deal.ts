import type { Rng } from './random.ts';

// 48 колонок формата 1 docs/CONTRACT.md, в том же порядке.
export const DEAL_COLUMNS = [
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

export type DealColumn = (typeof DEAL_COLUMNS)[number];
export type DealRow = Partial<Record<DealColumn, string | number>>;

/** Параметры распределения портфеля (доли — из задачи F7). */
export const MIX = {
  discriminator: [
    ['ANN', 35],
    ['RGM', 30],
    ['SIMPLE', 35],
  ] as const,
  float: 0.3,
  subsidy: 0.1,
  deposit: 0.1,
  amount: { min: 50_000, max: 20_000_000, median: 1_000_000, sigma: 1.1 },
  termYears: { min: 1, max: 30 },
  periods: ['1M', '3M', 'ENDDATE'] as const,
  conventions: ['ACT/365', 'ACT/ACT ISDA'] as const,
};

const DAY = 86_400_000;
const iso = (t: number) => new Date(t).toISOString().slice(0, 10);
const r2 = (x: number) => Math.round(x * 100) / 100;
const r4 = (x: number) => Math.round(x * 10_000) / 10_000;

function addYears(date: string, years: number): string {
  const d = new Date(date + 'T00:00:00Z');
  d.setUTCFullYear(d.getUTCFullYear() + years);
  return iso(d.getTime());
}

/** day_num: 1–28 или -1 (последний день месяца). */
const dayNum = (rng: Rng) => (rng.chance(0.15) ? -1 : rng.int(1, 28));

/** Лог-нормальная сумма в [min, max]. */
function amount(rng: Rng): number {
  const { min, max, median, sigma } = MIX.amount;
  const v = Math.exp(Math.log(median) + sigma * rng.normal());
  return r2(Math.min(max, Math.max(min, v)));
}

export function makeDeal(rng: Rng, index: number, reportDate: string): DealRow {
  const discriminator = rng.weighted(MIX.discriminator);
  const isFloat = rng.chance(MIX.float);
  const hasSub = rng.chance(MIX.subsidy);
  const isDeposit = rng.chance(MIX.deposit);
  const sign = isDeposit ? -1 : 1;

  // Срок 1–30 лет; сделка выдана не позже даты расчёта и ещё не погашена.
  const term = rng.int(MIX.termYears.min, MIX.termYears.max);
  const report = Date.parse(reportDate);
  const ageDays = Math.floor(rng.float() * Math.min(term * 365 - 1, 5 * 365));
  const dealDate = iso(report - ageDays * DAY);
  const maturity = addYears(dealDate, term);

  const start = amount(rng);
  // Для амортизируемых сделок остаток меньше выдачи пропорционально прошедшему сроку.
  const elapsed = ageDays / (term * 365);
  const current = discriminator === 'SIMPLE' ? start : r2(start * Math.max(0.05, 1 - elapsed));

  const interestPeriod = rng.pick(MIX.periods);
  const regularPeriod = discriminator === 'SIMPLE' ? 'ENDDATE' : rng.pick(['1M', '3M'] as const);
  const interestDay = dayNum(rng);

  const row: DealRow = {
    report_dttm: reportDate,
    discriminator,
    deal_id: `SYN${String(index + 1).padStart(8, '0')}`,
    currency_id: 'RUB',
    start_principal_amt: sign * start,
    current_principal_amt: sign * current,
    deal_dttm: dealDate,
    maturity_dttm: maturity,
    rate_type_cd: isFloat ? 'FLOAT' : 'FIXED',
    interest_rate: isFloat ? r4(rng.uniform(0.01, 0.05)) : r4(rng.uniform(0.03, 0.25)),
    interest_payment_rule_start_dttm: dealDate,
    interest_payment_rule_period_cd: interestPeriod,
    interest_payment_rule_day_num: interestDay,
    day_convention_cd: rng.pick(MIX.conventions),
    regular_payment_rule_start_dttm: regularPeriod === 'ENDDATE' ? '' : dealDate,
    regular_payment_rule_period_cd: regularPeriod,
    regular_payment_rule_day_num: regularPeriod === 'ENDDATE' ? '' : interestDay,
    product: isDeposit ? 'SYN_DEPOSIT' : `SYN_LOAN_${discriminator}`,
    deal_report_dttm: reportDate,
  };

  if (isFloat) {
    Object.assign(row, {
      base_indicator: 'CBKEY1M',
      base_indicator_rate: 0.08,
      repricing_rule_start_dttm: dealDate,
      repricing_rule_period_cd: rng.pick(['1M', '3M'] as const),
      repricing_rule_day_num: dayNum(rng),
      indicator_lag_cd: '-1M',
      floor_limit_interest_rate: rng.chance(0.5) ? r4(rng.uniform(0.04, 0.08)) : '',
      cap_limit_interest_rate: rng.chance(0.5) ? r4(rng.uniform(0.15, 0.3)) : '',
    } satisfies DealRow);
  }

  if (hasSub) {
    Object.assign(row, {
      sub_rate_type_cd: 'FLOAT',
      sub_interest_rate: r4(rng.uniform(0.01, 0.03)),
      sub_float_indicator: 'CBKEY1M',
      sub_coeff_index: 1,
      sub_payment_rule_period_cd: '1M',
      sub_payment_rule_day_num: dayNum(rng),
      sub_payment_rule_start_dttm: dealDate,
      sub_repricing_rule_start_dttm: dealDate,
      sub_repricing_rule_period_cd: '1M',
      sub_repricing_rule_day_num: 1,
      sub_indicator_lag_cd: '-1M',
    } satisfies DealRow);
  }
  return row;
}
