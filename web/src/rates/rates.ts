import Papa from 'papaparse';
import { isRealDate } from '../data/schemas';
import type { IsoDate, ValidationError } from '../data/types';

/** Ступенчатая функция индикатора: значение действует с даты до следующей даты (контракт, формат 2). */
export type Steps = [IsoDate, number][];

export interface RateFile {
  /** Ключ — индикатор, а если в файле есть колонка scenario, то «индикатор · сценарий». */
  series: Map<string, Steps>;
  errors: ValidationError[];
  rowCount: number;
}

// Принимаем формат контракта (indicator,date,rate) и формат кривых из плана спринта 3
// (rate_date,indicator_code,rate_value); колонка scenario (base/stress/…) — необязательная.
const COLUMN_ALIASES = {
  indicator: ['indicator', 'indicator_code'],
  date: ['date', 'rate_date'],
  rate: ['rate', 'rate_value'],
  scenario: ['scenario', 'scenario_cd'],
} as const;

const NUMBER = /^-?\d+(\.\d+)?([eE][-+]?\d+)?$/;

export function parseRates(text: string): RateFile {
  const parsed = Papa.parse<Record<string, string>>(text.trim(), {
    header: true,
    skipEmptyLines: 'greedy',
  });
  const fields = (parsed.meta.fields ?? []).map((f) => f.trim().replace(/^\uFEFF/, ''));
  const pick = (names: readonly string[]) => names.find((n) => fields.includes(n));
  const col = {
    indicator: pick(COLUMN_ALIASES.indicator),
    date: pick(COLUMN_ALIASES.date),
    rate: pick(COLUMN_ALIASES.rate),
    scenario: pick(COLUMN_ALIASES.scenario),
  };
  const errors: ValidationError[] = [];
  const series = new Map<string, Steps>();
  if (!col.indicator || !col.date || !col.rate) {
    errors.push({
      row: 0,
      field: '',
      message: 'ожидаются колонки indicator,date,rate или rate_date,indicator_code,rate_value',
    });
    return { series, errors, rowCount: parsed.data.length };
  }
  parsed.data.forEach((raw, i) => {
    const row = i + 1;
    const indicator = (raw[col.indicator!] ?? '').trim();
    const date = (raw[col.date!] ?? '').trim();
    const rate = (raw[col.rate!] ?? '').trim();
    const scenario = col.scenario ? (raw[col.scenario] ?? '').trim() : '';
    if (!indicator)
      return void errors.push({ row, field: col.indicator!, message: 'пустой индикатор' });
    if (!isRealDate(date))
      return void errors.push({ row, field: col.date!, message: 'ожидается дата YYYY-MM-DD' });
    if (!NUMBER.test(rate))
      return void errors.push({ row, field: col.rate!, message: 'ожидается число в долях' });
    const key = scenario ? `${indicator} · ${scenario}` : indicator;
    const list = series.get(key);
    if (list) list.push([date, Number(rate)]);
    else series.set(key, [[date, Number(rate)]]);
  });
  for (const list of series.values()) list.sort((a, b) => (a[0] < b[0] ? -1 : a[0] > b[0] ? 1 : 0));
  return { series, errors, rowCount: parsed.data.length };
}

/** Значение на дату: последняя точка не позже даты; null — дата раньше начала истории. */
export function valueAt(steps: Steps, date: IsoDate): number | null {
  let lo = 0;
  let hi = steps.length - 1;
  let found: number | null = null;
  while (lo <= hi) {
    const mid = (lo + hi) >> 1;
    if (steps[mid]![0] <= date) {
      found = steps[mid]![1];
      lo = mid + 1;
    } else {
      hi = mid - 1;
    }
  }
  return found;
}

/** Прогноз H3: после даты расчёта индикатор = значение на дату расчёта + сдвиг сценария (б.п.). */
export function forecastH3(
  steps: Steps,
  report: IsoDate,
  shiftBp: number,
  horizon: IsoDate,
): Steps {
  const base = valueAt(steps, report);
  if (base == null) return [];
  const v = base + shiftBp / 10_000;
  return [
    [report, v],
    [horizon, v],
  ];
}

/** История до даты расчёта включительно — то, что движок может знать на дату расчёта. */
export function knownAt(steps: Steps, report: IsoDate): Steps {
  return steps.filter(([d]) => d <= report);
}

/** Изменения ставки: даты, где значение отличается от предыдущего, — «ступеньки». */
export function changes(steps: Steps): Steps {
  return steps.filter(([, v], i) => i === 0 || v !== steps[i - 1]![1]);
}
