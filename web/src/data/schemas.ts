import { z } from 'zod';
import type { CashflowEvent, CsvKind, Deal, RowTypes, ValidationError } from './types';

z.config(z.locales.ru());

// CSV по контракту: пустое значение = null, даты YYYY-MM-DD, числа с точкой.
// Колонки описаны одной таблицей: из неё строятся и zod-схема (источник сообщений об ошибках),
// и быстрый разборщик для горячего пути — zod на 500 000 строк заметно медленнее.

type ColumnSpec =
  | { t: 'date' | 'num' | 'int' | 'uint' | 'str'; req?: true }
  | { t: 'enum'; values: readonly string[]; req?: true };

const date = { t: 'date' } as const;
const reqDate = { t: 'date', req: true } as const;
const num = { t: 'num' } as const;
const reqNum = { t: 'num', req: true } as const;
const int = { t: 'int' } as const;
const str = { t: 'str' } as const;
const reqStr = { t: 'str', req: true } as const;
const oneOf = (values: readonly string[], req = true): ColumnSpec =>
  req ? { t: 'enum', values, req } : { t: 'enum', values };

const PAYMENT_TYPES = [
  'interest_payment',
  'principal_payment',
  'close_payment',
  'repricing',
  'sub_payment',
  'sub_repricing',
  'prepayment',
] as const;
const DISCRIMINATORS = ['ANN', 'RGM', 'SIMPLE'] as const;

export const CASHFLOW_COLUMNS = {
  report_dttm: reqDate,
  deal_id: reqStr,
  event_order: { t: 'uint', req: true },
  payment_dttm: reqDate,
  payment_type: oneOf(PAYMENT_TYPES),
  stream: oneOf(['client', 'sub']),
  amount: reqNum,
  principal_before: num,
  principal_after: num,
  rate: num,
  base_rate: num,
  accrual_start_dttm: date,
  accrual_end_dttm: date,
  accrual_days: int,
  year_frac: num,
  discriminator: oneOf(DISCRIMINATORS, false),
} satisfies Record<keyof CashflowEvent, ColumnSpec>;

export const DEAL_COLUMNS = {
  report_dttm: reqDate,
  discriminator: oneOf(DISCRIMINATORS),
  deal_id: reqStr,
  currency_id: str,
  start_principal_amt: num,
  current_principal_amt: reqNum,
  deal_dttm: date,
  maturity_dttm: reqDate,
  rate_type_cd: oneOf(['FIXED', 'FLOAT']),
  interest_rate: num,
  base_indicator_rate: num,
  base_indicator: str,
  interest_payment_rule_start_dttm: date,
  interest_payment_rule_period_cd: str,
  interest_payment_rule_day_num: int,
  day_convention_cd: str,
  regular_payment_rule_start_dttm: date,
  regular_payment_rule_period_cd: str,
  regular_payment_rule_day_num: int,
  repricing_rule_start_dttm: date,
  repricing_rule_period_cd: str,
  repricing_rule_day_num: int,
  accrued_interest: num,
  product: str,
  behavior_model_cd: str,
  interest_payment: str,
  principal_payment: str,
  repricing_dttm_schedule: str,
  repricing_dttm_rate_schedule: str,
  repricing_dttm_rate_schedule_fixed: str,
  cap_limit_interest_rate: num,
  floor_limit_interest_rate: num,
  sub_rate_type_cd: str,
  sub_interest_rate: num,
  sub_float_indicator: str,
  sub_payment_rule_period_cd: str,
  sub_payment_rule_day_num: int,
  sub_payment_rule_start_dttm: date,
  sub_repricing_rule_start_dttm: date,
  sub_repricing_rule_period_cd: str,
  sub_repricing_rule_day_num: int,
  sub_cap_limit_interest_rate: num,
  sub_floor_limit_interest_rate: num,
  sub_coeff_index: num,
  deal_report_dttm: date,
  sub_repricing_rule_first_event_flg: str,
  indicator_lag_cd: str,
  sub_indicator_lag_cd: str,
} satisfies Record<keyof Deal, ColumnSpec>;

export const COLUMNS: Record<CsvKind, Record<string, ColumnSpec>> = {
  cashflow: CASHFLOW_COLUMNS,
  deals: DEAL_COLUMNS,
};

const ISO_DATE = /^(\d{4})-(\d{2})-(\d{2})$/;
const NUMBER = /^-?\d+(\.\d+)?([eE][-+]?\d+)?$/;
const INT = /^-?\d+$/;
const UINT = /^\d+$/;

export function isRealDate(s: string): boolean {
  const m = ISO_DATE.exec(s);
  if (!m) return false;
  const [y, mo, d] = [Number(m[1]), Number(m[2]), Number(m[3])];
  const dt = new Date(Date.UTC(y, mo - 1, d));
  return dt.getUTCFullYear() === y && dt.getUTCMonth() === mo - 1 && dt.getUTCDate() === d;
}

// --- zod-схемы ---

const empty = (v: unknown) => (v === undefined || v === '' ? null : v);

function zodColumn(spec: ColumnSpec): z.ZodType {
  let base: z.ZodType;
  switch (spec.t) {
    case 'date':
      base = z.string().refine(isRealDate, 'ожидается дата YYYY-MM-DD');
      break;
    case 'num':
      base = z.string().regex(NUMBER, 'ожидается число').transform(Number);
      break;
    case 'int':
      base = z.string().regex(INT, 'ожидается целое').transform(Number);
      break;
    case 'uint':
      base = z.string().regex(UINT, 'ожидается целое ≥ 0').transform(Number);
      break;
    case 'str':
      base = z.string();
      break;
    case 'enum':
      base = z.enum(spec.values as [string, ...string[]]);
      break;
  }
  return z.preprocess(empty, spec.req ? base : base.nullable());
}

const zodObject = <T>(columns: Record<string, ColumnSpec>) =>
  z.object(
    Object.fromEntries(Object.entries(columns).map(([k, s]) => [k, zodColumn(s)])),
  ) as unknown as z.ZodType<T>;

export const cashflowSchema = zodObject<CashflowEvent>(CASHFLOW_COLUMNS);
export const dealSchema = zodObject<Deal>(DEAL_COLUMNS);

export const SCHEMAS = { cashflow: cashflowSchema, deals: dealSchema };

// --- быстрый разбор ---

const INVALID = Symbol('invalid');

function fastColumn(spec: ColumnSpec): (v: string | undefined) => unknown {
  const req = spec.req === true;
  const values = spec.t === 'enum' ? new Set(spec.values) : null;
  return (v) => {
    if (v === undefined || v === '') return req ? INVALID : null;
    switch (spec.t) {
      case 'date':
        return isRealDate(v) ? v : INVALID;
      case 'num':
        return NUMBER.test(v) ? Number(v) : INVALID;
      case 'int':
        return INT.test(v) ? Number(v) : INVALID;
      case 'uint':
        return UINT.test(v) ? Number(v) : INVALID;
      case 'str':
        return v;
      case 'enum':
        return values!.has(v) ? v : INVALID;
    }
  };
}

const fastParsers = Object.fromEntries(
  Object.entries(COLUMNS).map(([kind, columns]) => [
    kind,
    Object.entries(columns).map(([k, s]) => [k, fastColumn(s)] as const),
  ]),
) as Record<CsvKind, (readonly [string, (v: string | undefined) => unknown])[]>;

export type RowResult<K extends CsvKind> =
  { ok: true; value: RowTypes[K] } | { ok: false; errors: ValidationError[] };

/** Проверяет одну строку CSV (объект «колонка → строка»). row — номер строки данных с 1. */
export function parseRow<K extends CsvKind>(
  kind: K,
  raw: Record<string, string | undefined>,
  row: number,
): RowResult<K> {
  const out: Record<string, unknown> = {};
  let valid = true;
  for (const [key, parse] of fastParsers[kind]) {
    const v = parse(raw[key]);
    if (v === INVALID) {
      valid = false;
      break;
    }
    out[key] = v;
  }
  if (valid) return { ok: true, value: out as unknown as RowTypes[K] };

  // Медленный путь только для плохих строк: zod даёт понятные сообщения по всем полям.
  const res = SCHEMAS[kind].safeParse(raw);
  if (res.success) return { ok: true, value: res.data as RowTypes[K] };
  return {
    ok: false,
    errors: res.error.issues.map((issue) => ({
      row,
      field: String(issue.path[0] ?? ''),
      message: issue.message,
    })),
  };
}
