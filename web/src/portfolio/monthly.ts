import type { CashflowEvent } from '../data/types';

export type FlowKind = 'principal' | 'interest' | 'subsidy';

export interface MonthBucket {
  /** YYYY-MM */
  month: string;
  /** Активы (суммы > 0) и пассивы (суммы < 0) раздельно, чтобы не взаимозачитывались. */
  assets: Record<FlowKind, number>;
  liabilities: Record<FlowKind, number>;
}

const BODY = new Set(['principal_payment', 'close_payment', 'prepayment']);

export function flowKind(e: CashflowEvent): FlowKind | null {
  if (e.stream === 'sub') return e.payment_type === 'sub_payment' ? 'subsidy' : null;
  if (e.payment_type === 'interest_payment') return 'interest';
  if (BODY.has(e.payment_type)) return 'principal';
  return null;
}

const zero = (): Record<FlowKind, number> => ({ principal: 0, interest: 0, subsidy: 0 });

/** Поток по месяцам payment_dttm. Месяцы без платежей внутри диапазона заполняются нулями. */
export function monthlyFlow(events: Iterable<CashflowEvent>): MonthBucket[] {
  const map = new Map<string, MonthBucket>();
  for (const e of events) {
    const kind = flowKind(e);
    if (!kind || e.amount === 0) continue;
    const month = e.payment_dttm.slice(0, 7);
    let b = map.get(month);
    if (!b) {
      b = { month, assets: zero(), liabilities: zero() };
      map.set(month, b);
    }
    (e.amount > 0 ? b.assets : b.liabilities)[kind] += e.amount;
  }
  const months = [...map.keys()].sort();
  if (months.length === 0) return [];
  const out: MonthBucket[] = [];
  let [y, m] = months[0]!.split('-').map(Number) as [number, number];
  const last = months.at(-1)!;
  for (;;) {
    const key = `${y}-${String(m).padStart(2, '0')}`;
    out.push(map.get(key) ?? { month: key, assets: zero(), liabilities: zero() });
    if (key >= last) break;
    m += 1;
    if (m > 12) [y, m] = [y + 1, 1];
  }
  return out;
}
