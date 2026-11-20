import { compareCashflows, type CompareRow, type RowStatus } from '../compare/compare';
import { indexByDeal } from '../data/store';
import type { CashflowEvent, IsoDate } from '../data/types';

/** Итоги сделки в одном прогоне. */
export interface DealTotals {
  interest: number;
  subsidy: number;
  principal: number;
  prepayment: number;
  /** Платежей по телу: principal_payment + prepayment + close_payment. */
  bodyPayments: number;
  closeDate: IsoDate | null;
  events: number;
}

export interface ScenarioDeal {
  dealId: string;
  before: DealTotals | null;
  after: DealTotals | null;
  /** Δ процентов клиента, «стало − было». */
  dInterest: number;
  dSubsidy: number;
  /** Сдвиг даты закрытия, дней (отрицательный — закрылась раньше). */
  closeShiftDays: number | null;
  changedRows: number;
}

/** Строка сравнения: 'same' — совпало до 0,005 ₽, 'changed' — сумма или остаток другие,
 *  'added' — новое событие (досрочка, пересмотр), 'removed' — события больше нет. */
export type ScenarioStatus = 'same' | 'changed' | 'added' | 'removed';

export const SCENARIO_LABEL: Record<ScenarioStatus, string> = {
  same: 'без изменений',
  changed: 'изменилось',
  added: 'новое',
  removed: 'убрано',
};

export interface ScenarioRow extends Omit<CompareRow, 'status' | 'golden' | 'engine'> {
  status: ScenarioStatus;
  before: CashflowEvent | null;
  after: CashflowEvent | null;
}

const STATUS: Record<RowStatus, ScenarioStatus> = {
  match: 'same',
  diff: 'changed',
  missing: 'removed',
  extra: 'added',
};

// Сценарий сравнивается строже сверки: любое изменение больше полукопейки видно.
const TOL = 0.005;
const DAY = 86_400_000;

export function dealTotals(events: CashflowEvent[]): DealTotals {
  const t: DealTotals = {
    interest: 0,
    subsidy: 0,
    principal: 0,
    prepayment: 0,
    bodyPayments: 0,
    closeDate: null,
    events: events.length,
  };
  for (const e of events) {
    if (e.stream === 'sub') {
      if (e.payment_type === 'sub_payment') t.subsidy += e.amount;
    } else if (e.payment_type === 'interest_payment') {
      t.interest += e.amount;
    } else if (
      e.payment_type === 'principal_payment' ||
      e.payment_type === 'close_payment' ||
      e.payment_type === 'prepayment'
    ) {
      t.principal += e.amount;
      t.bodyPayments += 1;
      if (e.payment_type === 'prepayment') t.prepayment += e.amount;
      if (e.payment_type === 'close_payment') t.closeDate = e.payment_dttm;
    }
  }
  return t;
}

/** Построчное «было / стало» одной сделки (или нескольких): ключ сделка + дата + тип + поток. */
export function diffRows(before: CashflowEvent[], after: CashflowEvent[]): ScenarioRow[] {
  return compareCashflows(after, before, TOL).rows.map(({ golden, engine, status, ...rest }) => ({
    ...rest,
    status: STATUS[status],
    before: golden,
    after: engine,
  }));
}

/** Сводка сценария по всем сделкам, которые есть хотя бы в одном прогоне. */
export function scenarioSummary(before: CashflowEvent[], after: CashflowEvent[]): ScenarioDeal[] {
  const b = indexByDeal(before);
  const a = indexByDeal(after);
  const ids = [...new Set([...b.keys(), ...a.keys()])].sort((x, y) =>
    x.localeCompare(y, 'ru', { numeric: true }),
  );
  return ids.map((dealId) => {
    const eb = b.get(dealId) ?? [];
    const ea = a.get(dealId) ?? [];
    const tb = eb.length ? dealTotals(eb) : null;
    const ta = ea.length ? dealTotals(ea) : null;
    const changedRows = diffRows(eb, ea).filter((r) => r.status !== 'same').length;
    const shift =
      tb?.closeDate && ta?.closeDate
        ? Math.round((Date.parse(ta.closeDate) - Date.parse(tb.closeDate)) / DAY)
        : null;
    return {
      dealId,
      before: tb,
      after: ta,
      dInterest: (ta?.interest ?? 0) - (tb?.interest ?? 0),
      dSubsidy: (ta?.subsidy ?? 0) - (tb?.subsidy ?? 0),
      closeShiftDays: shift,
      changedRows,
    };
  });
}

/** Остаток тела во времени: точки [дата, остаток после последнего события дня]. */
export function balancePath(events: CashflowEvent[], start: IsoDate): [IsoDate, number][] {
  const out: [IsoDate, number][] = [];
  const first = events.find((e) => e.principal_before != null);
  if (first) out.push([start, first.principal_before!]);
  for (const e of events) {
    if (e.principal_after == null) continue;
    const last = out.at(-1);
    if (last && last[0] === e.payment_dttm) last[1] = e.principal_after;
    else out.push([e.payment_dttm, e.principal_after]);
  }
  return out;
}
