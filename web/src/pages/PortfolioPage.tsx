import { createColumnHelper } from '@tanstack/react-table';
import { useMemo, useState } from 'react';
import { useNavigate } from 'react-router';
import { DropZone } from '../components/DropZone';
import { EChart, type EChartOption } from '../components/EChart';
import { ErrorList } from '../components/ErrorList';
import { ProgressBar } from '../components/ProgressBar';
import { StatTile } from '../components/StatTile';
import ui from '../components/ui.module.css';
import { VirtualTable, type ColumnMeta } from '../components/VirtualTable';
import type { CsvKind } from '../data/types';
import { fmtDate, fmtInt, fmtMoney } from '../format';
import { monthlyFlow, type FlowKind } from '../portfolio/monthly';
import type { LoadState } from '../state/DataContext';
import { useData } from '../state/useData';
import styles from './Page.module.css';

interface DealRow {
  dealId: string;
  discriminator: string;
  rateType: string;
  eventCount: number;
  principalTotal: number;
  interestTotal: number;
  subsidyTotal: number;
  prepaymentTotal: number;
  /** Коды замечаний движка из errors.csv. */
  issues: string[];
  hasError: boolean;
  lastDate: string;
  nonZero: boolean;
}

const col = createColumnHelper<DealRow>();
const money = { width: 'minmax(130px, 1fr)', align: 'right' } satisfies ColumnMeta;

const COLUMNS = [
  col.accessor('dealId', {
    header: 'deal_id',
    meta: { width: 'minmax(140px, 1.4fr)' },
    cell: (c) => <span className={ui.mono}>{c.getValue()}</span>,
  }),
  col.accessor('discriminator', { header: 'Тип', meta: { width: '90px' } }),
  col.accessor('rateType', { header: 'Ставка', meta: { width: '80px' } }),
  col.accessor('eventCount', {
    header: 'Событий',
    meta: { width: '90px', align: 'right' },
    cell: (c) => fmtInt(c.getValue()),
  }),
  col.accessor('principalTotal', {
    header: 'Тело',
    meta: money,
    cell: (c) => fmtMoney(c.getValue()),
  }),
  col.accessor('interestTotal', {
    header: 'Проценты',
    meta: money,
    cell: (c) => fmtMoney(c.getValue()),
  }),
  col.accessor('subsidyTotal', {
    header: 'Субсидия',
    meta: money,
    cell: (c) => fmtMoney(c.getValue()),
  }),
  col.accessor('lastDate', {
    header: 'Последний платёж',
    meta: { width: '140px' },
    cell: (c) => fmtDate(c.getValue()),
  }),
  col.accessor((r) => r.issues.join(' '), {
    id: 'issues',
    header: 'Замечания движка',
    meta: { width: 'minmax(150px, 1.2fr)' },
    cell: (c) => {
      const r = c.row.original;
      if (!r.issues.length) return <span className={ui.muted}>—</span>;
      return (
        <span className={r.hasError ? ui.warn : ui.muted} title={r.issues.join(', ')}>
          {r.hasError ? '✗ ' : '⚠ '}
          {r.issues.join(', ')}
        </span>
      );
    },
  }),
  col.accessor('nonZero', {
    header: 'Остаток',
    meta: { width: '80px' },
    cell: (c) =>
      c.getValue() ? (
        <span className={ui.warn} title="Итоговый остаток не равен 0">
          ⚠
        </span>
      ) : (
        <span className={ui.ok} title="Итоговый остаток 0">
          ✓
        </span>
      ),
  }),
];

const FLOW_SERIES: { kind: FlowKind; name: string; color: string }[] = [
  { kind: 'principal', name: 'Тело', color: '#3E5BA8' },
  { kind: 'interest', name: 'Проценты', color: '#B4541A' },
  { kind: 'subsidy', name: 'Субсидия', color: '#8A4F9E' },
];

function LoadStatus({ load }: { load?: LoadState }) {
  if (!load) return null;
  return (
    <div>
      {load.status === 'loading' && <ProgressBar percent={load.progress} />}
      {load.status === 'failed' && <div className={ui.warn}>Ошибка: {load.message}</div>}
      {load.status === 'done' && (
        <div className={ui.muted}>
          Строк: {fmtInt(load.rowCount)}, валидных: {fmtInt(load.rowCount - load.errorCount)}
        </div>
      )}
      <ErrorList errors={load.errors} total={load.errorCount} />
    </div>
  );
}

export function PortfolioPage() {
  const { cashflow, deals, issues, loads, loadFile, loadDemo } = useData();
  const navigate = useNavigate();
  const [query, setQuery] = useState('');
  const [disc, setDisc] = useState('');

  const rows = useMemo<DealRow[]>(() => {
    if (!cashflow) return [];
    const out: DealRow[] = cashflow.summaries.map((s) => {
      const deal = deals?.get(s.dealId);
      const first = cashflow.byDeal.get(s.dealId)?.[0];
      const dealIssues = issues?.get(s.dealId) ?? [];
      return {
        dealId: s.dealId,
        discriminator: deal?.discriminator ?? first?.discriminator ?? '—',
        rateType: deal?.rate_type_cd ?? '—',
        eventCount: s.eventCount,
        principalTotal: s.principalTotal,
        interestTotal: s.interestTotal,
        subsidyTotal: s.subsidyTotal,
        prepaymentTotal: s.prepaymentTotal,
        issues: dealIssues.map((i) => i.code),
        hasError: dealIssues.some((i) => i.severity === 'error'),
        lastDate: s.lastDate ?? '',
        nonZero: s.finalBalanceNonZero,
      };
    });
    // Сделки, отклонённые движком (error), в кэшфлоу не попадают — показываем их строкой с замечанием.
    if (issues) {
      const seen = new Set(out.map((r) => r.dealId));
      for (const [id, list] of issues) {
        if (!id || seen.has(id) || !list.some((i) => i.severity === 'error')) continue;
        const deal = deals?.get(id);
        out.push({
          dealId: id,
          discriminator: deal?.discriminator ?? '—',
          rateType: deal?.rate_type_cd ?? '—',
          eventCount: 0,
          principalTotal: 0,
          interestTotal: 0,
          subsidyTotal: 0,
          prepaymentTotal: 0,
          issues: list.map((i) => i.code),
          hasError: true,
          lastDate: '',
          nonZero: false,
        });
      }
    }
    return out;
  }, [cashflow, deals, issues]);

  const totals = useMemo(() => {
    const t = {
      events: 0,
      principal: 0,
      interest: 0,
      subsidy: 0,
      prepayment: 0,
      nonZero: 0,
      rejected: 0,
      warned: 0,
    };
    for (const r of rows) {
      t.events += r.eventCount;
      t.principal += r.principalTotal;
      t.interest += r.interestTotal;
      t.subsidy += r.subsidyTotal;
      t.prepayment += r.prepaymentTotal;
      if (r.nonZero) t.nonZero += 1;
      if (r.hasError) t.rejected += 1;
      else if (r.issues.length) t.warned += 1;
    }
    return t;
  }, [rows]);

  const discriminators = useMemo(
    () => [...new Set(rows.map((r) => r.discriminator))].sort(),
    [rows],
  );

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    return rows.filter(
      (r) => (!disc || r.discriminator === disc) && (!q || r.dealId.toLowerCase().includes(q)),
    );
  }, [rows, query, disc]);

  const chartOption = useMemo<EChartOption | null>(() => {
    if (!cashflow) return null;
    const buckets = monthlyFlow(
      (function* () {
        for (const list of cashflow.byDeal.values()) yield* list;
      })(),
    );
    const months = buckets.map((b) => `${b.month.slice(5)}.${b.month.slice(0, 4)}`);
    const series = FLOW_SERIES.flatMap(({ kind, name, color }) =>
      (['assets', 'liabilities'] as const).map((side) => ({
        name,
        type: 'bar',
        stack: 'flow',
        itemStyle: { color, opacity: side === 'assets' ? 1 : 0.7 },
        emphasis: { focus: 'series' },
        data: buckets.map((b) => Math.round(b[side][kind] * 100) / 100),
      })),
    );
    return {
      grid: { left: 90, right: 20, top: 40, bottom: 60 },
      legend: { top: 0, data: FLOW_SERIES.map((s) => s.name) },
      tooltip: {
        trigger: 'axis',
        axisPointer: { type: 'shadow' },
        valueFormatter: (v: number) => fmtMoney(v),
      },
      xAxis: { type: 'category', data: months },
      yAxis: {
        type: 'value',
        axisLabel: {
          formatter: (v: number) =>
            new Intl.NumberFormat('ru-RU', { notation: 'compact' }).format(v),
        },
      },
      dataZoom:
        buckets.length > 36 ? [{ type: 'inside' }, { type: 'slider', height: 18, bottom: 8 }] : [],
      series,
    };
  }, [cashflow]);

  const onFiles = (kind: CsvKind) => (files: File[]) => {
    const f = files[0];
    if (f) void loadFile(kind, f, f.name);
  };

  const busy = Object.values(loads).some((l) => l?.status === 'loading');

  return (
    <section className={styles.page}>
      <div className={styles.toolbar}>
        <h1 className={styles.title}>Портфель</h1>
        <div className={styles.row}>
          <button
            className={ui.buttonGhost}
            onClick={() => void loadDemo('samples')}
            disabled={busy}
          >
            Загрузить демо
          </button>
          <button
            className={ui.button}
            onClick={() => void loadDemo('engine')}
            disabled={busy}
            title="Вывод cfgen по 55 тестовым сделкам: кэшфлоу, сделки и errors.csv"
          >
            Результат движка: 55 сделок
          </button>
        </div>
      </div>

      <div className={styles.grid2}>
        <div>
          <DropZone
            title="Перетащите кэшфлоу (CSV)"
            hint="результат калькулятора, формат 3 контракта"
            fileName={loads.cashflow?.fileName}
            onFiles={onFiles('cashflow')}
          />
          <LoadStatus load={loads.cashflow} />
        </div>
        <div>
          <DropZone
            title="Перетащите сделки (CSV)"
            hint="необязательно: тип ставки и параметры сделки"
            fileName={loads.deals?.fileName}
            onFiles={onFiles('deals')}
          />
          <LoadStatus load={loads.deals} />
        </div>
        <div>
          <DropZone
            title="Перетащите errors.csv"
            hint="необязательно: замечания движка по сделкам"
            fileName={loads.errors?.fileName}
            onFiles={onFiles('errors')}
          />
          <LoadStatus load={loads.errors} />
        </div>
      </div>

      {cashflow && (
        <>
          <div className={ui.stats}>
            <StatTile label="сделок" value={fmtInt(rows.length)} />
            <StatTile label="событий" value={fmtInt(totals.events)} />
            <StatTile label="сумма тела, ₽" value={fmtMoney(totals.principal)} />
            <StatTile label="сумма процентов, ₽" value={fmtMoney(totals.interest)} />
            <StatTile label="сумма субсидии, ₽" value={fmtMoney(totals.subsidy)} />
            {totals.prepayment !== 0 && (
              <StatTile label="досрочно погашено, ₽" value={fmtMoney(totals.prepayment)} />
            )}
            <StatTile
              label="с ненулевым остатком"
              value={fmtInt(totals.nonZero)}
              danger={totals.nonZero > 0}
            />
            {issues && (
              <StatTile
                label="отклонено / с предупреждением"
                value={`${fmtInt(totals.rejected)} / ${fmtInt(totals.warned)}`}
                danger={totals.rejected > 0}
              />
            )}
          </div>

          <div className={ui.panel}>
            <h2 className={ui.panelTitle}>Поток по месяцам</h2>
            <p className={styles.hint}>Активы над осью, пассивы (депозиты) под осью.</p>
            {chartOption && <EChart option={chartOption} />}
          </div>

          <div className={styles.toolbar}>
            <h2 className={ui.panelTitle}>Сделки</h2>
            <div className={styles.row}>
              <input
                className={ui.input}
                placeholder="Поиск по deal_id"
                value={query}
                onChange={(e) => setQuery(e.target.value)}
              />
              <select className={ui.input} value={disc} onChange={(e) => setDisc(e.target.value)}>
                <option value="">Все типы</option>
                {discriminators.map((d) => (
                  <option key={d} value={d}>
                    {d}
                  </option>
                ))}
              </select>
              <span className={ui.muted}>
                {fmtInt(filtered.length)} из {fmtInt(rows.length)}
              </span>
            </div>
          </div>
          <VirtualTable
            data={filtered}
            columns={COLUMNS}
            height={Math.min(520, 38 + filtered.length * 34)}
            onRowClick={(r) => navigate(`/deal/${encodeURIComponent(r.dealId)}`)}
            empty="Нет сделок по фильтру"
          />
        </>
      )}
    </section>
  );
}
