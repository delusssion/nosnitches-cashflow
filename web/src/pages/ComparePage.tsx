import { createColumnHelper } from '@tanstack/react-table';
import { useMemo, useState } from 'react';
import { DropZone } from '../components/DropZone';
import { ErrorList } from '../components/ErrorList';
import { ProgressBar } from '../components/ProgressBar';
import { StatTile } from '../components/StatTile';
import ui from '../components/ui.module.css';
import { VirtualTable, type ColumnMeta } from '../components/VirtualTable';
import {
  compareCashflows,
  compareReportCsv,
  STATUS_LABEL,
  type CompareRow,
  type DealCompare,
} from '../compare/compare';
import cs from '../compare/Compare.module.css';
import { downloadText } from '../data/exportCsv';
import { fmtDate, fmtInt, fmtMoney } from '../format';
import type { CompareSide, SideState } from '../state/CompareContext';
import { useCompare } from '../state/useCompare';
import styles from './Page.module.css';

const dcol = createColumnHelper<DealCompare>();
const num = { width: '110px', align: 'right' } satisfies ColumnMeta;

const DEAL_COLUMNS = [
  dcol.accessor('ok', {
    header: 'Статус',
    meta: { width: '150px' },
    cell: (c) =>
      c.getValue() ? (
        <span className={cs.ok}>✓ совпало</span>
      ) : (
        <span className={cs.bad}>✗ есть отличия</span>
      ),
  }),
  dcol.accessor('dealId', {
    header: 'deal_id',
    meta: { width: 'minmax(140px, 1fr)' },
    cell: (c) => <span className={ui.mono}>{c.getValue()}</span>,
  }),
  dcol.accessor('total', { header: 'Строк', meta: num, cell: (c) => fmtInt(c.getValue()) }),
  dcol.accessor('match', { header: 'Совпало', meta: num, cell: (c) => fmtInt(c.getValue()) }),
  dcol.accessor('diff', { header: 'Расхождений', meta: num, cell: (c) => fmtInt(c.getValue()) }),
  dcol.accessor('missing', {
    header: 'Нет в движке',
    meta: num,
    cell: (c) => fmtInt(c.getValue()),
  }),
  dcol.accessor('extra', { header: 'Лишнее', meta: num, cell: (c) => fmtInt(c.getValue()) }),
  dcol.accessor('maxAbsDelta', {
    header: 'Макс |Δ|',
    meta: { width: '120px', align: 'right' },
    cell: (c) => fmtMoney(c.getValue()),
  }),
];

const rcol = createColumnHelper<CompareRow>();
const money = { width: 'minmax(110px, 1fr)', align: 'right' } satisfies ColumnMeta;

function Delta({ v, tol }: { v: number | null; tol: number }) {
  if (v == null) return <span className={ui.muted}>—</span>;
  return <span className={Math.abs(v) > tol + 1e-9 ? cs.delta : undefined}>{fmtMoney(v)}</span>;
}

function rowColumns(tol: number) {
  return [
    rcol.accessor('dealId', {
      header: 'deal_id',
      meta: { width: 'minmax(120px, 1fr)' },
      cell: (c) => <span className={ui.mono}>{c.getValue()}</span>,
    }),
    rcol.accessor('date', {
      header: 'Дата',
      meta: { width: '100px' },
      cell: (c) => fmtDate(c.getValue()),
    }),
    rcol.accessor('type', {
      header: 'Тип',
      meta: { width: '170px' },
      cell: (c) => <span className={ui.mono}>{c.getValue()}</span>,
    }),
    rcol.accessor('stream', { header: 'Поток', meta: { width: '70px' } }),
    rcol.accessor('status', {
      header: 'Статус',
      meta: { width: '140px' },
      cell: (c) => <span className={cs[c.getValue()]}>{STATUS_LABEL[c.getValue()]}</span>,
    }),
    rcol.accessor((r) => r.golden?.amount ?? null, {
      id: 'gAmount',
      header: 'Эталон: сумма',
      meta: money,
      cell: (c) => fmtMoney(c.getValue()),
    }),
    rcol.accessor((r) => r.engine?.amount ?? null, {
      id: 'eAmount',
      header: 'Движок: сумма',
      meta: money,
      cell: (c) => fmtMoney(c.getValue()),
    }),
    rcol.accessor('dAmount', {
      header: 'Δ сумма',
      meta: money,
      cell: (c) => <Delta v={c.getValue()} tol={tol} />,
    }),
    rcol.accessor((r) => r.golden?.principal_after ?? null, {
      id: 'gPrincipal',
      header: 'Эталон: остаток',
      meta: money,
      cell: (c) => fmtMoney(c.getValue()),
    }),
    rcol.accessor((r) => r.engine?.principal_after ?? null, {
      id: 'ePrincipal',
      header: 'Движок: остаток',
      meta: money,
      cell: (c) => fmtMoney(c.getValue()),
    }),
    rcol.accessor('dPrincipal', {
      header: 'Δ остаток',
      meta: money,
      cell: (c) => <Delta v={c.getValue()} tol={tol} />,
    }),
  ];
}

function SideStatus({ side }: { side: SideState }) {
  return (
    <div>
      {side.status === 'loading' && <ProgressBar percent={side.progress} />}
      {side.status === 'failed' && <div className={ui.warn}>Ошибка: {side.message}</div>}
      {side.status === 'done' && (
        <div className={ui.muted}>
          Файлов: {side.files.length}, событий: {fmtInt(side.events.length)}
        </div>
      )}
      <ErrorList errors={side.errors} total={side.errorCount} />
    </div>
  );
}

const DEMO = 'samples/cashflow_sample.csv';

export function ComparePage() {
  const { engine, golden, tolerance, setTolerance, loadSide } = useCompare();
  const [toleranceText, setToleranceText] = useState(String(tolerance));
  const [onlyBad, setOnlyBad] = useState(true);
  const [dealFilter, setDealFilter] = useState<string | null>(null);

  const result = useMemo(
    () =>
      engine.status === 'done' && golden.status === 'done'
        ? compareCashflows(engine.events, golden.events, tolerance)
        : null,
    [engine, golden, tolerance],
  );

  const stats = useMemo(() => {
    if (!result) return null;
    const s = {
      deals: result.deals.length,
      okDeals: 0,
      rows: result.rows.length,
      match: 0,
      bad: 0,
      maxDelta: 0,
    };
    for (const d of result.deals) {
      if (d.ok) s.okDeals += 1;
      s.maxDelta = Math.max(s.maxDelta, d.maxAbsDelta);
    }
    for (const r of result.rows) {
      if (r.status === 'match') s.match += 1;
      else s.bad += 1;
    }
    return s;
  }, [result]);

  const shownRows = useMemo(
    () =>
      result?.rows.filter(
        (r) => (!onlyBad || r.status !== 'match') && (!dealFilter || r.dealId === dealFilter),
      ) ?? [],
    [result, onlyBad, dealFilter],
  );
  const columns = useMemo(() => rowColumns(tolerance), [tolerance]);

  const onFiles = (side: CompareSide) => (files: File[]) =>
    void loadSide(
      side,
      files.map((f) => ({ blob: f, name: f.name })),
    );

  const loadDemo = async () => {
    const blob = await (await fetch(DEMO)).blob();
    await Promise.all([
      loadSide('engine', [{ blob, name: 'cashflow_sample.csv' }]),
      loadSide('golden', [{ blob, name: 'cashflow_sample.csv (демо, не эталон)' }]),
    ]);
  };

  return (
    <section className={styles.page}>
      <div className={styles.toolbar}>
        <h1 className={styles.title}>Сверка с эталоном</h1>
        <div className={styles.row}>
          <label className={cs.tolerance}>
            Допуск, ₽
            <input
              className={ui.input}
              type="number"
              step="0.001"
              min="0"
              value={toleranceText}
              onChange={(e) => {
                setToleranceText(e.target.value);
                const v = Number(e.target.value);
                if (e.target.value !== '' && Number.isFinite(v) && v >= 0) setTolerance(v);
              }}
            />
          </label>
          <button className={ui.buttonGhost} onClick={() => void loadDemo()}>
            Демо-сверка
          </button>
          {result && (
            <button
              className={ui.button}
              onClick={() => downloadText(compareReportCsv(result), 'compare_report.csv')}
            >
              Экспорт отчёта CSV
            </button>
          )}
        </div>
      </div>

      <div className={styles.grid2}>
        <div>
          <DropZone
            title="Результат движка (CSV)"
            hint="один файл кэшфлоу, формат 3"
            fileName={engine.files.join(', ') || undefined}
            onFiles={onFiles('engine')}
          />
          <SideStatus side={engine} />
        </div>
        <div>
          <DropZone
            title="Эталон (CSV)"
            hint="testdata/golden/deal_<id>.csv — можно несколько файлов, или один общий"
            fileName={
              golden.files.length > 3
                ? `${golden.files.length} файлов`
                : golden.files.join(', ') || undefined
            }
            multiple
            onFiles={onFiles('golden')}
          />
          <SideStatus side={golden} />
        </div>
      </div>

      {result && stats && (
        <>
          <div className={stats.bad === 0 ? cs.verdictOk : cs.verdictBad}>
            {stats.bad === 0
              ? `✓ Все ${fmtInt(stats.deals)} сделок совпали с эталоном при допуске ${fmtMoney(tolerance)} ₽`
              : `✗ Отличия в ${fmtInt(stats.deals - stats.okDeals)} из ${fmtInt(stats.deals)} сделок`}
          </div>
          <div className={ui.stats}>
            <StatTile
              label="сделок совпало"
              value={`${fmtInt(stats.okDeals)} из ${fmtInt(stats.deals)}`}
            />
            <StatTile label="строк сравнено" value={fmtInt(stats.rows)} />
            <StatTile label="строк совпало" value={fmtInt(stats.match)} />
            <StatTile label="строк с отличиями" value={fmtInt(stats.bad)} danger={stats.bad > 0} />
            <StatTile label="макс |Δ|, ₽" value={fmtMoney(stats.maxDelta)} danger={stats.bad > 0} />
          </div>

          <h2 className={ui.panelTitle}>Сводка по сделкам</h2>
          <VirtualTable
            data={result.deals}
            columns={DEAL_COLUMNS}
            height={Math.min(360, 38 + result.deals.length * 34)}
            onRowClick={(d) => setDealFilter((cur) => (cur === d.dealId ? null : d.dealId))}
            rowStyle={(d) => ({
              background: d.ok ? 'var(--c-accent-light)' : '#FBE4E2',
              fontWeight: d.dealId === dealFilter ? 700 : undefined,
            })}
          />

          <div className={styles.toolbar}>
            <h2 className={ui.panelTitle}>
              {onlyBad ? 'Расхождения' : 'Все строки'}
              {dealFilter && (
                <>
                  {' '}
                  по сделке <span className={ui.mono}>{dealFilter}</span>{' '}
                  <button className={ui.buttonGhost} onClick={() => setDealFilter(null)}>
                    сбросить
                  </button>
                </>
              )}
            </h2>
            <label className={cs.tolerance}>
              <input
                type="checkbox"
                checked={onlyBad}
                onChange={(e) => setOnlyBad(e.target.checked)}
              />
              только отличия
            </label>
          </div>
          <VirtualTable
            data={shownRows}
            columns={columns}
            height={Math.min(480, 38 + Math.max(shownRows.length, 2) * 34)}
            rowStyle={(r) => (r.status === 'match' ? undefined : { background: '#FDF1F0' })}
            empty={onlyBad ? 'Расхождений нет ✓' : 'Нет строк'}
          />
        </>
      )}
    </section>
  );
}
