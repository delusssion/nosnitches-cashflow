import { createColumnHelper } from '@tanstack/react-table';
import { useMemo } from 'react';
import { DropZone } from '../components/DropZone';
import { EChart, type EChartOption } from '../components/EChart';
import { ErrorList } from '../components/ErrorList';
import { StatTile } from '../components/StatTile';
import ui from '../components/ui.module.css';
import { VirtualTable } from '../components/VirtualTable';
import { fmtDate, fmtInt, fmtRate } from '../format';
import { changes, forecastH3, knownAt, valueAt, type Steps } from '../rates/rates';
import rs from '../rates/Rates.module.css';
import type { LoadedRates } from '../state/RatesContext';
import { useData } from '../state/useData';
import { useRates } from '../state/useRates';
import styles from './Page.module.css';

const KEY_RATE = 'samples/key_rate.csv';
const INDICATOR = 'CBKEY1M';
const CURVE_COLORS = ['#3E5BA8', '#B42318', '#8A4F9E', '#1D7A8C', '#B4541A'];

interface Change {
  date: string;
  value: number;
  delta: number | null;
}
const col = createColumnHelper<Change>();
const CHANGE_COLUMNS = [
  col.accessor('date', {
    header: 'Дата',
    meta: { width: '120px' },
    cell: (c) => fmtDate(c.getValue()),
  }),
  col.accessor('value', {
    header: 'Ключевая ставка',
    meta: { width: '1fr', align: 'right' },
    cell: (c) => fmtRate(c.getValue()),
  }),
  col.accessor('delta', {
    header: 'Изменение',
    meta: { width: '1fr', align: 'right' },
    cell: (c) => {
      const v = c.getValue();
      if (v == null) return '—';
      const bp = Math.round(v * 10_000);
      return <span className={bp > 0 ? rs.up : rs.down}>{bp > 0 ? `+${bp}` : bp} б.п.</span>;
    },
  }),
];

function FileStatus({ file }: { file: LoadedRates | null }) {
  if (!file) return null;
  return (
    <div>
      <div className={ui.muted}>
        Строк: {fmtInt(file.rowCount)}, рядов: {[...file.series.keys()].join(', ') || '—'}
      </div>
      <ErrorList errors={file.errors} total={file.errors.length} />
    </div>
  );
}

const addYears = (d: string, years: number) => `${Number(d.slice(0, 4)) + years}${d.slice(4)}`;

export function RatesPage() {
  const {
    history,
    curves,
    reportDate,
    shiftBp,
    setReportDate,
    setShiftBp,
    loadHistory,
    loadCurves,
  } = useRates();
  const { deals } = useData();
  const reportFromDeals = deals?.values().next().value?.report_dttm;

  const steps: Steps = useMemo(
    () => history?.series.get(INDICATOR) ?? history?.series.values().next().value ?? [],
    [history],
  );
  const horizon = addYears(reportDate, 3);
  const known = useMemo(() => knownAt(steps, reportDate), [steps, reportDate]);
  const future = useMemo(() => steps.filter(([d]) => d >= reportDate), [steps, reportDate]);
  const forecast = useMemo(
    () => forecastH3(steps, reportDate, shiftBp, horizon),
    [steps, reportDate, shiftBp, horizon],
  );
  const atReport = valueAt(steps, reportDate);

  const changeRows = useMemo<Change[]>(() => {
    const c = changes(known);
    return c
      .map(([date, value], i) => ({ date, value, delta: i ? value - c[i - 1]![1] : null }))
      .reverse();
  }, [known]);

  const option = useMemo<EChartOption | null>(() => {
    if (!steps.length && !curves) return null;
    const series: object[] = [
      {
        name: 'КС ЦБ: известна на дату расчёта',
        type: 'line',
        step: 'end',
        symbol: 'none',
        lineStyle: { color: '#0E6B57', width: 2 },
        itemStyle: { color: '#0E6B57' },
        data: known,
        markLine: {
          silent: true,
          symbol: 'none',
          lineStyle: { type: 'dashed', color: '#14211D' },
          label: { formatter: `дата расчёта ${fmtDate(reportDate)}`, position: 'insideEndTop' },
          data: [{ xAxis: reportDate }],
        },
      },
      {
        name: 'КС ЦБ: фактически после (движок не видит)',
        type: 'line',
        step: 'end',
        symbol: 'none',
        lineStyle: { color: '#55665F', width: 1, opacity: 0.6 },
        itemStyle: { color: '#55665F' },
        data: future,
      },
      {
        name: `Прогноз H3${shiftBp ? ` ${shiftBp > 0 ? '+' : ''}${shiftBp} б.п.` : ''}`,
        type: 'line',
        symbol: 'none',
        lineStyle: { type: 'dashed', color: '#B4541A', width: 2 },
        itemStyle: { color: '#B4541A' },
        data: forecast,
      },
    ];
    [...(curves?.series ?? new Map<string, Steps>())].forEach(([name, data], i) => {
      series.push({
        name: `Кривая: ${name}`,
        type: 'line',
        step: 'end',
        symbol: 'circle',
        symbolSize: 4,
        lineStyle: { color: CURVE_COLORS[i % CURVE_COLORS.length], width: 2 },
        itemStyle: { color: CURVE_COLORS[i % CURVE_COLORS.length] },
        data,
      });
    });
    return {
      animation: false,
      grid: { left: 70, right: 30, top: 70, bottom: 60 },
      legend: { top: 0, type: 'scroll' },
      tooltip: { trigger: 'axis', valueFormatter: (v: number) => fmtRate(v) },
      xAxis: { type: 'time', axisLabel: { formatter: '{MM}.{yyyy}', hideOverlap: true } },
      yAxis: { type: 'value', axisLabel: { formatter: (v: number) => fmtRate(v) } },
      dataZoom: [{ type: 'inside' }, { type: 'slider', height: 18, bottom: 8 }],
      series,
    };
  }, [steps, known, future, forecast, curves, reportDate, shiftBp]);

  return (
    <section className={styles.page}>
      <div className={styles.toolbar}>
        <h1 className={styles.title}>Ставки</h1>
        <button
          className={ui.button}
          onClick={async () => loadHistory(await (await fetch(KEY_RATE)).blob(), 'key_rate.csv')}
        >
          КС ЦБ из testdata
        </button>
      </div>
      <p className={styles.hint}>
        Индикатор плавающих сделок — ключевая ставка ЦБ. После даты расчёта движок не видит
        фактическую историю: берёт значение на дату расчёта плюс сдвиг сценария (гипотеза H3).
        Кривые команды 2 (базовая и стресс) подключаются в спринте 3 — их CSV можно наложить на
        график.
      </p>

      <div className={styles.grid2}>
        <div>
          <DropZone
            title="История индикатора (CSV)"
            hint="indicator,date,rate — формат 2 контракта"
            fileName={history?.fileName}
            onFiles={(f) => f[0] && void loadHistory(f[0], f[0].name)}
          />
          <FileStatus file={history} />
        </div>
        <div>
          <DropZone
            title="Кривые команды 2 (CSV)"
            hint="indicator,date,rate или rate_date,indicator_code,rate_value; колонка scenario — необязательно"
            fileName={curves?.fileName}
            onFiles={(f) => f[0] && void loadCurves(f[0], f[0].name)}
          />
          <FileStatus file={curves} />
        </div>
      </div>

      <div className={rs.controls}>
        <label className={rs.control}>
          Дата расчёта
          <input
            className={ui.input}
            type="date"
            value={reportDate}
            onChange={(e) => e.target.value && setReportDate(e.target.value)}
          />
        </label>
        {reportFromDeals && reportFromDeals !== reportDate && (
          <button className={ui.buttonGhost} onClick={() => setReportDate(reportFromDeals)}>
            как в сделках: {fmtDate(reportFromDeals)}
          </button>
        )}
        <label className={rs.control}>
          Сдвиг сценария, б.п.
          <input
            className={ui.input}
            type="number"
            step="25"
            value={shiftBp}
            onChange={(e) => setShiftBp(Number(e.target.value) || 0)}
          />
        </label>
        {[-200, 0, 200].map((bp) => (
          <button
            key={bp}
            className={bp === shiftBp ? ui.button : ui.buttonGhost}
            onClick={() => setShiftBp(bp)}
          >
            {bp === 0 ? 'база' : `${bp > 0 ? '+' : ''}${bp} б.п.`}
          </button>
        ))}
      </div>

      {steps.length > 0 && (
        <div className={ui.stats}>
          <StatTile label={`КС на ${fmtDate(reportDate)}`} value={fmtRate(atReport)} />
          <StatTile
            label="прогноз движка после даты расчёта"
            value={atReport == null ? '—' : fmtRate(atReport + shiftBp / 10_000)}
          />
          <StatTile label="изменений КС до даты расчёта" value={fmtInt(changeRows.length)} />
          <StatTile
            label="история"
            value={`${fmtDate(steps[0]![0])} – ${fmtDate(steps.at(-1)![0])}`}
          />
        </div>
      )}

      {option && (
        <div className={ui.panel}>
          <EChart option={option} height={420} />
        </div>
      )}

      {changeRows.length > 0 && (
        <>
          <h2 className={ui.panelTitle}>Решения по ключевой ставке до даты расчёта</h2>
          <VirtualTable
            data={changeRows}
            columns={CHANGE_COLUMNS}
            height={Math.min(420, 38 + changeRows.length * 34)}
          />
        </>
      )}
    </section>
  );
}
