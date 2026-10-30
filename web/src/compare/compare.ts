import type { CashflowEvent } from '../data/types';

export type RowStatus = 'match' | 'diff' | 'missing' | 'extra';

export const STATUS_LABEL: Record<RowStatus, string> = {
  match: 'совпало',
  diff: 'расхождение',
  missing: 'нет в движке',
  extra: 'лишнее в движке',
};

export interface CompareRow {
  dealId: string;
  date: string;
  type: CashflowEvent['payment_type'];
  stream: CashflowEvent['stream'];
  /** Номер повтора ключа (с 0), если ключ встречается в сделке несколько раз. */
  dup: number;
  status: RowStatus;
  golden: CashflowEvent | null;
  engine: CashflowEvent | null;
  dAmount: number | null;
  dPrincipal: number | null;
}

export interface DealCompare {
  dealId: string;
  total: number;
  match: number;
  diff: number;
  missing: number;
  extra: number;
  maxAbsDelta: number;
  ok: boolean;
}

export interface CompareResult {
  rows: CompareRow[];
  deals: DealCompare[];
  tolerance: number;
}

// Сумма 100.01 − 100 даёт 0.010000000000005 > 0.01: сравниваем с запасом на ошибку double.
const EPS = 1e-9;

const keyOf = (e: CashflowEvent) =>
  `${e.deal_id}\u0000${e.payment_dttm}\u0000${e.payment_type}\u0000${e.stream}`;

/** Группирует события по ключу контракта; дубли ключа — в порядке следования. */
function group(events: CashflowEvent[]): Map<string, CashflowEvent[]> {
  const map = new Map<string, CashflowEvent[]>();
  for (const e of events) {
    const k = keyOf(e);
    const list = map.get(k);
    if (list) list.push(e);
    else map.set(k, [e]);
  }
  return map;
}

const delta = (a: number | null | undefined, b: number | null | undefined) =>
  a == null && b == null ? 0 : (a ?? 0) - (b ?? 0);

/**
 * Сверка результата движка с эталоном по ключу deal_id + payment_dttm + payment_type + stream
 * (docs/CONTRACT.md, формат 4). При дублях ключа строки сопоставляются по порядковому номеру.
 */
export function compareCashflows(
  engine: CashflowEvent[],
  golden: CashflowEvent[],
  tolerance = 0.01,
): CompareResult {
  const eng = group(engine);
  const gold = group(golden);
  const keys = new Set([...gold.keys(), ...eng.keys()]);
  const rows: CompareRow[] = [];

  for (const k of keys) {
    const g = gold.get(k) ?? [];
    const e = eng.get(k) ?? [];
    const sample = (g[0] ?? e[0])!;
    for (let i = 0; i < Math.max(g.length, e.length); i++) {
      const ge = g[i] ?? null;
      const ee = e[i] ?? null;
      let status: RowStatus;
      let dAmount: number | null = null;
      let dPrincipal: number | null = null;
      if (!ee) status = 'missing';
      else if (!ge) status = 'extra';
      else {
        dAmount = ee.amount - ge.amount;
        dPrincipal = delta(ee.principal_after, ge.principal_after);
        status =
          Math.abs(dAmount) <= tolerance + EPS && Math.abs(dPrincipal) <= tolerance + EPS
            ? 'match'
            : 'diff';
      }
      rows.push({
        dealId: sample.deal_id,
        date: sample.payment_dttm,
        type: sample.payment_type,
        stream: sample.stream,
        dup: i,
        status,
        golden: ge,
        engine: ee,
        dAmount,
        dPrincipal,
      });
    }
  }

  rows.sort(
    (a, b) =>
      a.dealId.localeCompare(b.dealId) ||
      a.date.localeCompare(b.date) ||
      (a.golden ?? a.engine)!.event_order - (b.golden ?? b.engine)!.event_order,
  );

  const byDeal = new Map<string, DealCompare>();
  for (const r of rows) {
    let d = byDeal.get(r.dealId);
    if (!d) {
      d = {
        dealId: r.dealId,
        total: 0,
        match: 0,
        diff: 0,
        missing: 0,
        extra: 0,
        maxAbsDelta: 0,
        ok: true,
      };
      byDeal.set(r.dealId, d);
    }
    d.total += 1;
    d[r.status] += 1;
    if (r.status !== 'match') d.ok = false;
    d.maxAbsDelta = Math.max(d.maxAbsDelta, Math.abs(r.dAmount ?? 0), Math.abs(r.dPrincipal ?? 0));
  }

  return { rows, deals: [...byDeal.values()], tolerance };
}

const REPORT_COLUMNS = [
  'deal_id',
  'payment_dttm',
  'payment_type',
  'stream',
  'dup',
  'status',
  'golden_amount',
  'engine_amount',
  'delta_amount',
  'golden_principal_after',
  'engine_principal_after',
  'delta_principal_after',
];

const n2 = (v: number | null | undefined) => (v == null ? '' : v.toFixed(2));

/** Отчёт сверки в CSV: одна строка на сопоставленную пару. */
export function compareReportCsv(result: CompareResult): string {
  const lines = result.rows.map((r) =>
    [
      r.dealId,
      r.date,
      r.type,
      r.stream,
      r.dup,
      r.status,
      n2(r.golden?.amount),
      n2(r.engine?.amount),
      n2(r.dAmount),
      n2(r.golden?.principal_after),
      n2(r.engine?.principal_after),
      n2(r.dPrincipal),
    ]
      .map((v) => {
        const s = String(v);
        return /[",\n]/.test(s) ? `"${s.replaceAll('"', '""')}"` : s;
      })
      .join(','),
  );
  return [REPORT_COLUMNS.join(','), ...lines].join('\n') + '\n';
}
