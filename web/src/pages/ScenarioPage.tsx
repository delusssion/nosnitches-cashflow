import { createColumnHelper } from '@tanstack/react-table';
import { useMemo, useState } from 'react';
import cs from '../compare/Compare.module.css';
import { DropZone } from '../components/DropZone';
import { EChart, type EChartOption } from '../components/EChart';
import { ErrorList } from '../components/ErrorList';
import { ProgressBar } from '../components/ProgressBar';
import { StatTile } from '../components/StatTile';
import ui from '../components/ui.module.css';
import { VirtualTable, type ColumnMeta } from '../components/VirtualTable';
import { PAYMENT_TYPE_INFO } from '../deal/paymentTypes';
import { fmtDate, fmtInt, fmtMoney, fmtRate } from '../format';
import {
  balancePath,
  diffRows,
  SCENARIO_LABEL,
  scenarioSummary,
  type ScenarioDeal,
  type ScenarioRow,
  type ScenarioStatus,
} from '../scenario/scenario';
import ss from '../scenario/Scenario.module.css';
import type { SideState } from '../state/useCashflowSide';
import { useScenario } from '../state/useScenario';
import styles from './Page.module.css';

const DEMO_BEFORE = 'samples/scenario/cashflow_before.csv';
const DEMO_AFTER = 'samples/scenario/cashflow_after.csv';

const signed = (v: number) => (v > 0 ? '+' : '') + fmtMoney(v);
const deltaClass = (v: number) => (Math.abs(v) < 0.005 ? ui.muted : v > 0 ? ss.up : ss.down);

const dcol = createColumnHelper<ScenarioDeal>();
const money = { width: 'minmax(120px, 1fr)', align: 'right' } satisfies ColumnMeta;

const DEAL_COLUMNS = [
  dcol.accessor('dealId', {
    header: 'deal_id',
    meta: { width: '100px' },
    cell: (c) => <span className={ui.mono}>{c.getValue()}</span>,
  }),
  dcol.accessor((d) => d.before?.interest ?? 0, {
    id: 'ib',
    header: 'Проценты: было',
    meta: money,
    cell: (c) => fmtMoney(c.getValue()),
  }),
  dcol.accessor((d) => d.after?.interest ?? 0, {
    id: 'ia',
    header: 'Проценты: стало',
    meta: money,
    cell: (c) => fmtMoney(c.getValue()),
  }),
  dcol.accessor('dInterest', {
    header: 'Δ процентов',
    meta: money,
    cell: (c) => <span className={deltaClass(c.getValue())}>{signed(c.getValue())}</span>,
  }),
  dcol.accessor((d) => d.after?.prepayment ?? 0, {
    id: 'pp',
    header: 'Досрочно',
    meta: money,
    cell: (c) => (c.getValue() ? fmtMoney(c.getValue()) : <span className={ui.muted}>—</span>),
  }),
  dcol.accessor((d) => d.after?.closeDate ?? '', {
    id: 'close',
    header: 'Закрытие: стало',
    meta: { width: '130px' },
    cell: (c) => fmtDate(c.getValue()),
  }),
  dcol.accessor('closeShiftDays', {
    header: 'Сдвиг, дн.',
    meta: { width: '90px', align: 'right' },
    cell: (c) => {
      const v = c.getValue();
      if (v == null) return '—';
      return (
        <span className={v === 0 ? ui.muted : v < 0 ? ss.down : ss.up}>{v > 0 ? `+${v}` : v}</span>
      );
    },
  }),
  dcol.accessor('changedRows', {
    header: 'Строк изм.',
    meta: { width: '90px', align: 'right' },
    cell: (c) => fmtInt(c.getValue()),
  }),
];

const STATUS_CLASS: Record<ScenarioStatus, string | undefined> = {
  same: undefined,
  changed: ss.changed,
  added: ss.added,
  removed: ss.removed,
};

const rcol = createColumnHelper<ScenarioRow>();

const ROW_COLUMNS = [
  rcol.accessor('date', {
    header: 'Дата',
    meta: { width: '110px' },
    cell: (c) => fmtDate(c.getValue()),
  }),
  rcol.accessor('type', {
    header: 'Тип',
    meta: { width: '200px' },
    cell: (c) => {
      const info = PAYMENT_TYPE_INFO[c.getValue()];
      return (
        <span className={ui.badge} style={{ color: info.color, background: info.bg }}>
          {c.getValue()}
        </span>
      );
    },
  }),
  rcol.accessor('status', {
    header: 'Что с ним',
    meta: { width: '130px' },
    cell: (c) => <span className={STATUS_CLASS[c.getValue()]}>{SCENARIO_LABEL[c.getValue()]}</span>,
  }),
  rcol.accessor((r) => r.before?.amount ?? null, {
    id: 'ab',
    header: 'Было: сумма',
    meta: money,
    cell: (c) => fmtMoney(c.getValue()),
  }),
  rcol.accessor((r) => r.after?.amount ?? null, {
    id: 'aa',
    header: 'Стало: сумма',
    meta: money,
    cell: (c) => fmtMoney(c.getValue()),
  }),
  rcol.accessor((r) => (r.after?.amount ?? 0) - (r.before?.amount ?? 0), {
    id: 'da',
    header: 'Δ суммы',
    meta: money,
    cell: (c) => <span className={deltaClass(c.getValue())}>{signed(c.getValue())}</span>,
  }),
  rcol.accessor((r) => r.after?.principal_after ?? r.before?.principal_after ?? null, {
    id: 'bal',
    header: 'Остаток: стало',
    meta: money,
    cell: (c) => fmtMoney(c.getValue()),
  }),
  rcol.accessor((r) => r.after?.rate ?? r.before?.rate ?? null, {
    id: 'rate',
    header: 'Ставка: стало',
    meta: { width: '110px', align: 'right' },
    cell: (c) => fmtRate(c.getValue()),
  }),
];

function SideStatus({ side }: { side: SideState }) {
  return (
    <div>
      {side.status === 'loading' && <ProgressBar percent={side.progress} />}
      {side.status === 'failed' && <div className={ui.warn}>Ошибка: {side.message}</div>}
      {side.status === 'done' && (
        <div className={ui.muted}>Событий: {fmtInt(side.events.length)}</div>
      )}
      <ErrorList errors={side.errors} total={side.errorCount} />
    </div>
  );
}

export function ScenarioPage() {
  const { before, after, loadBefore, loadAfter } = useScenario();
  const [selected, setSelected] = useState<string | null>(null);
  const [onlyChanged, setOnlyChanged] = useState(true);

  const ready = before.status === 'done' && after.status === 'done';
  const summary = useMemo(
    () => (ready ? scenarioSummary(before.events, after.events) : []),
    [ready, before, after],
  );
  // По умолчанию — сделка с самым большим эффектом на проценты.
  const biggest = summary.reduce<ScenarioDeal | null>(
    (best, d) => (!best || Math.abs(d.dInterest) > Math.abs(best.dInterest) ? d : best),
    null,
  );
  const dealId = selected ?? biggest?.dealId ?? null;
  const deal = summary.find((d) => d.dealId === dealId) ?? null;

  const totals = useMemo(() => {
    const t = { changed: 0, dInterest: 0, dSubsidy: 0, prepayment: 0 };
    for (const d of summary) {
      if (d.changedRows > 0) t.changed += 1;
      t.dInterest += d.dInterest;
      t.dSubsidy += d.dSubsidy;
      t.prepayment += d.after?.prepayment ?? 0;
    }
    return t;
  }, [summary]);

  const dealEvents = useMemo(() => {
    if (!dealId) return { b: [], a: [] };
    return {
      b: before.events.filter((e) => e.deal_id === dealId),
      a: after.events.filter((e) => e.deal_id === dealId),
    };
  }, [before, after, dealId]);

  const rows = useMemo(() => {
    const all = diffRows(dealEvents.b, dealEvents.a);
    return onlyChanged ? all.filter((r) => r.status !== 'same') : all;
  }, [dealEvents, onlyChanged]);

  const chart = useMemo<EChartOption | null>(() => {
    const start = dealEvents.b[0]?.report_dttm ?? dealEvents.a[0]?.report_dttm;
    if (!start) return null;
    const pathB = balancePath(dealEvents.b, start);
    const pathA = balancePath(dealEvents.a, start);
    const prepay = dealEvents.a
      .filter((e) => e.payment_type === 'prepayment')
      .map((e) => ({ value: [e.payment_dttm, e.principal_after], amount: e.amount }));
    return {
      animation: false,
      grid: { left: 90, right: 30, top: 40, bottom: 50 },
      legend: { top: 0, data: ['Было', 'Стало', 'Досрочно'] },
      tooltip: {
        trigger: 'axis',
        valueFormatter: (v: number) => fmtMoney(v),
      },
      xAxis: { type: 'time', axisLabel: { formatter: '{dd}.{MM}.{yyyy}', hideOverlap: true } },
      yAxis: {
        type: 'value',
        axisLabel: {
          formatter: (v: number) =>
            new Intl.NumberFormat('ru-RU', { notation: 'compact' }).format(v),
        },
      },
      dataZoom: [{ type: 'inside' }, { type: 'slider', height: 18, bottom: 6 }],
      series: [
        {
          name: 'Было',
          type: 'line',
          step: 'end',
          symbol: 'none',
          lineStyle: { type: 'dashed', color: '#55665F', width: 2 },
          itemStyle: { color: '#55665F' },
          data: pathB,
        },
        {
          name: 'Стало',
          type: 'line',
          step: 'end',
          symbol: 'circle',
          symbolSize: 5,
          lineStyle: { color: '#0E6B57', width: 2 },
          itemStyle: { color: '#0E6B57' },
          areaStyle: { color: 'rgba(14,107,87,0.08)' },
          data: pathA,
        },
        {
          name: 'Досрочно',
          type: 'scatter',
          symbol: 'pin',
          symbolSize: 34,
          itemStyle: { color: '#1D7A8C' },
          label: {
            show: true,
            formatter: (p: { data: { amount: number } }) => fmtMoney(p.data.amount),
            position: 'top',
            color: '#1D7A8C',
            fontWeight: 600,
          },
          data: prepay,
        },
      ],
    };
  }, [dealEvents]);

  const loadDemo = async () => {
    const get = async (path: string) => ({
      blob: await (await fetch(path)).blob(),
      name: path.split('/').pop()!,
    });
    setSelected(null);
    await Promise.all([loadBefore([await get(DEMO_BEFORE)]), loadAfter([await get(DEMO_AFTER)])]);
  };

  return (
    <section className={styles.page}>
      <div className={styles.toolbar}>
        <h1 className={styles.title}>Сценарий: было / стало</h1>
        <button
          className={ui.button}
          onClick={() => void loadDemo()}
          title="Вывод cfgen: сделка 22 — досрочка 250 000 ₽, сделка 7 — ставка 15 → 20 %, сделка 25 — cap 10 → 20 %"
        >
          Демо-сценарий
        </button>
      </div>
      <p className={styles.hint}>
        Два прогона движка: базовый и с изменёнными параметрами сделки, досрочным погашением или
        другим сценарием ставок. Сравнение — по ключу сделка + дата + тип + поток.
      </p>

      <div className={styles.grid2}>
        <div>
          <DropZone
            title="Было (CSV)"
            hint="базовый кэшфлоу движка"
            fileName={before.files.join(', ') || undefined}
            onFiles={(f) => void loadBefore(f.map((x) => ({ blob: x, name: x.name })))}
          />
          <SideStatus side={before} />
        </div>
        <div>
          <DropZone
            title="Стало (CSV)"
            hint="кэшфлоу сценария"
            fileName={after.files.join(', ') || undefined}
            onFiles={(f) => void loadAfter(f.map((x) => ({ blob: x, name: x.name })))}
          />
          <SideStatus side={after} />
        </div>
      </div>

      {ready && (
        <>
          <div className={ui.stats}>
            <StatTile
              label="сделок изменилось"
              value={`${fmtInt(totals.changed)} из ${fmtInt(summary.length)}`}
            />
            <StatTile label="Δ процентов по портфелю, ₽" value={signed(totals.dInterest)} />
            <StatTile label="Δ субсидии, ₽" value={signed(totals.dSubsidy)} />
            <StatTile label="досрочно погашено, ₽" value={fmtMoney(totals.prepayment)} />
          </div>

          <h2 className={ui.panelTitle}>Сделки</h2>
          <VirtualTable
            data={summary}
            columns={DEAL_COLUMNS}
            height={Math.min(360, 38 + summary.length * 34)}
            onRowClick={(d) => setSelected(d.dealId)}
            rowStyle={(d) => ({
              background: d.dealId === dealId ? 'var(--c-accent-light)' : undefined,
              fontWeight: d.dealId === dealId ? 600 : undefined,
            })}
          />

          {deal && (
            <>
              <h2 className={ui.panelTitle}>
                Сделка <span className={ui.mono}>{deal.dealId}</span>
              </h2>
              <div className={ui.stats}>
                <StatTile
                  label="проценты: было → стало, ₽"
                  value={`${fmtMoney(deal.before?.interest ?? 0)} → ${fmtMoney(deal.after?.interest ?? 0)}`}
                />
                <StatTile
                  label="Δ процентов, ₽"
                  value={signed(deal.dInterest)}
                  danger={deal.dInterest > 0.005}
                />
                <StatTile
                  label="закрытие: было → стало"
                  value={`${fmtDate(deal.before?.closeDate)} → ${fmtDate(deal.after?.closeDate)}`}
                />
                <StatTile
                  label="платежей по телу: было → стало"
                  value={`${fmtInt(deal.before?.bodyPayments ?? 0)} → ${fmtInt(deal.after?.bodyPayments ?? 0)}`}
                />
              </div>
              <div className={ui.panel}>
                <h2 className={ui.panelTitle}>Остаток долга</h2>
                {chart && <EChart option={chart} height={340} />}
              </div>
              <div className={styles.toolbar}>
                <h2 className={ui.panelTitle}>{onlyChanged ? 'Что изменилось' : 'Все события'}</h2>
                <label className={cs.tolerance}>
                  <input
                    type="checkbox"
                    checked={onlyChanged}
                    onChange={(e) => setOnlyChanged(e.target.checked)}
                  />
                  только изменения
                </label>
              </div>
              <VirtualTable
                data={rows}
                columns={ROW_COLUMNS}
                height={Math.min(520, 38 + Math.max(rows.length, 2) * 34)}
                empty="Изменений нет"
              />
            </>
          )}
        </>
      )}
    </section>
  );
}
