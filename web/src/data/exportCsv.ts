import { CASHFLOW_COLUMNS } from './schemas';
import type { CashflowEvent } from './types';

const COLUMNS = Object.keys(CASHFLOW_COLUMNS) as (keyof CashflowEvent)[];

const cell = (v: unknown) => {
  if (v === null || v === undefined) return '';
  const s =
    typeof v === 'number' && !Number.isInteger(v) ? String(Math.round(v * 1e10) / 1e10) : String(v);
  return /[",\n]/.test(s) ? `"${s.replaceAll('"', '""')}"` : s;
};

/** События в CSV формата 3 контракта. Суммы — до 2 знаков, как в выходе калькулятора. */
export function cashflowToCsv(events: CashflowEvent[]): string {
  const money = new Set<keyof CashflowEvent>(['amount', 'principal_before', 'principal_after']);
  const lines = events.map((e) =>
    COLUMNS.map((c) => {
      const v = e[c];
      return money.has(c) && typeof v === 'number' ? v.toFixed(2) : cell(v);
    }).join(','),
  );
  return [COLUMNS.join(','), ...lines].join('\n') + '\n';
}

export function downloadText(text: string, fileName: string) {
  const url = URL.createObjectURL(new Blob([text], { type: 'text/csv;charset=utf-8' }));
  const a = document.createElement('a');
  a.href = url;
  a.download = fileName;
  a.click();
  URL.revokeObjectURL(url);
}
