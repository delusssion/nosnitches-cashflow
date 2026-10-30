import { EVENT_DAY_ORDER, type CashflowEvent, type IsoDate } from './types';

/** Порог «ненулевого» итогового остатка. */
export const BALANCE_EPS = 0.005;

export interface DealSummary {
  dealId: string;
  eventCount: number;
  /** Сумма тела: principal_payment + close_payment (+ prepayment). */
  principalTotal: number;
  /** Проценты клиента (interest_payment, stream=client). */
  interestTotal: number;
  /** Субсидия (stream=sub). */
  subsidyTotal: number;
  lastDate: IsoDate | null;
  finalBalance: number | null;
  finalBalanceNonZero: boolean;
}

/** Сортировка событий сделки: по дате, затем по event_order, затем по порядку типов в день. */
export function compareEvents(a: CashflowEvent, b: CashflowEvent): number {
  if (a.payment_dttm !== b.payment_dttm) return a.payment_dttm < b.payment_dttm ? -1 : 1;
  if (a.event_order !== b.event_order) return a.event_order - b.event_order;
  return EVENT_DAY_ORDER[a.payment_type] - EVENT_DAY_ORDER[b.payment_type];
}

/** Индекс событий по deal_id; события внутри сделки отсортированы. */
export function indexByDeal(events: Iterable<CashflowEvent>): Map<string, CashflowEvent[]> {
  const index = new Map<string, CashflowEvent[]>();
  for (const e of events) {
    const list = index.get(e.deal_id);
    if (list) list.push(e);
    else index.set(e.deal_id, [e]);
  }
  for (const list of index.values()) list.sort(compareEvents);
  return index;
}

const BODY_TYPES = new Set(['principal_payment', 'close_payment', 'prepayment']);

/** Сводка по одной сделке; events должны быть отсортированы (см. indexByDeal). */
export function summarizeDeal(dealId: string, events: CashflowEvent[]): DealSummary {
  let principalTotal = 0;
  let interestTotal = 0;
  let subsidyTotal = 0;
  for (const e of events) {
    if (e.stream === 'sub') {
      subsidyTotal += e.amount;
    } else if (e.payment_type === 'interest_payment') {
      interestTotal += e.amount;
    } else if (BODY_TYPES.has(e.payment_type)) {
      principalTotal += e.amount;
    }
  }
  const last = events.at(-1);
  const finalBalance = last?.principal_after ?? null;
  return {
    dealId,
    eventCount: events.length,
    principalTotal,
    interestTotal,
    subsidyTotal,
    lastDate: last?.payment_dttm ?? null,
    finalBalance,
    finalBalanceNonZero: finalBalance !== null && Math.abs(finalBalance) > BALANCE_EPS,
  };
}

export interface CashflowStore {
  byDeal: Map<string, CashflowEvent[]>;
  summaries: DealSummary[];
}

export function buildStore(events: Iterable<CashflowEvent>): CashflowStore {
  const byDeal = indexByDeal(events);
  const summaries = Array.from(byDeal, ([id, list]) => summarizeDeal(id, list));
  return { byDeal, summaries };
}
