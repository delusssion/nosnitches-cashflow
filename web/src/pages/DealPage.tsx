import { createColumnHelper } from '@tanstack/react-table';
import { useMemo, useState } from 'react';
import { Link, useParams } from 'react-router';
import { EChart } from '../components/EChart';
import ui from '../components/ui.module.css';
import { VirtualTable, type ColumnMeta } from '../components/VirtualTable';
import { cashflowToCsv, downloadText } from '../data/exportCsv';
import type { CashflowEvent, PaymentType } from '../data/types';
import { DealParams } from '../deal/DealParams';
import dealStyles from '../deal/Deal.module.css';
import { PAYMENT_TYPE_INFO, PAYMENT_TYPES } from '../deal/paymentTypes';
import { timelineOption } from '../deal/timeline';
import { fmtDate, fmtInt, fmtMoney, fmtRate } from '../format';
import { useData } from '../state/useData';
import styles from './Page.module.css';

const col = createColumnHelper<CashflowEvent>();
const money = { width: 'minmax(120px, 1fr)', align: 'right' } satisfies ColumnMeta;

function TypeBadge({ type }: { type: PaymentType }) {
  const info = PAYMENT_TYPE_INFO[type];
  return (
    <span
      className={ui.badge}
      style={{ color: info.color, background: info.bg }}
      title={info.label}
    >
      {type}
    </span>
  );
}

function buildColumns(totals: { amount: number; count: number }) {
  return [
    col.accessor('payment_dttm', {
      header: 'Дата',
      meta: { width: '110px', footer: `Итого: ${fmtInt(totals.count)}` },
      cell: (c) => fmtDate(c.getValue()),
    }),
    col.accessor('payment_type', {
      header: 'Тип',
      meta: { width: '170px' },
      cell: (c) => <TypeBadge type={c.getValue()} />,
    }),
    col.accessor('stream', { header: 'Поток', meta: { width: '70px' } }),
    col.accessor('amount', {
      header: 'Сумма',
      meta: { ...money, footer: fmtMoney(totals.amount) },
      cell: (c) => fmtMoney(c.getValue()),
    }),
    col.accessor('principal_before', {
      header: 'Остаток до',
      meta: money,
      cell: (c) => fmtMoney(c.getValue()),
    }),
    col.accessor('principal_after', {
      header: 'Остаток после',
      meta: money,
      cell: (c) => fmtMoney(c.getValue()),
    }),
    col.accessor('rate', {
      header: 'Ставка',
      meta: { width: '90px', align: 'right' },
      cell: (c) => fmtRate(c.getValue()),
    }),
    col.display({
      id: 'accrual',
      header: 'Период начисления',
      meta: { width: '190px' },
      cell: (c) =>
        c.row.original.accrual_start_dttm
          ? `${fmtDate(c.row.original.accrual_start_dttm)} – ${fmtDate(c.row.original.accrual_end_dttm)}`
          : '',
    }),
    col.accessor('accrual_days', {
      header: 'Дней',
      meta: { width: '60px', align: 'right' },
      cell: (c) => c.getValue() ?? '',
    }),
  ];
}

function NoDeal() {
  const { cashflow, loadDemo } = useData();
  if (!cashflow) {
    return (
      <section className={styles.page}>
        <h1 className={styles.title}>Сделка</h1>
        <p className={styles.hint}>Данные не загружены.</p>
        <div className={styles.row}>
          <button className={ui.button} onClick={() => void loadDemo()}>
            Загрузить демо
          </button>
          <Link to="/">или загрузить файлы на экране «Портфель»</Link>
        </div>
      </section>
    );
  }
  const ids = [...cashflow.byDeal.keys()];
  return (
    <section className={styles.page}>
      <h1 className={styles.title}>Сделка</h1>
      <p className={styles.hint}>Выберите сделку (или кликните строку на экране «Портфель»):</p>
      <div className={dealStyles.dealList}>
        {ids.slice(0, 50).map((id) => (
          <Link key={id} to={`/deal/${encodeURIComponent(id)}`} className={ui.mono}>
            {id}
          </Link>
        ))}
        {ids.length > 50 && <span className={ui.muted}>… и ещё {fmtInt(ids.length - 50)}</span>}
      </div>
    </section>
  );
}

export function DealPage() {
  const { id } = useParams();
  const { cashflow, deals } = useData();
  const [hidden, setHidden] = useState<Set<PaymentType>>(new Set());

  const events = useMemo(() => (id && cashflow?.byDeal.get(id)) || [], [cashflow, id]);
  const deal = id ? deals?.get(id) : undefined;
  const present = useMemo(
    () => PAYMENT_TYPES.filter((t) => events.some((e) => e.payment_type === t)),
    [events],
  );
  const visible = useMemo(
    () => events.filter((e) => !hidden.has(e.payment_type)),
    [events, hidden],
  );
  const totals = useMemo(
    () => ({ amount: visible.reduce((s, e) => s + e.amount, 0), count: visible.length }),
    [visible],
  );
  const columns = useMemo(() => buildColumns(totals), [totals]);
  const option = useMemo(
    () => timelineOption(events, deal?.report_dttm ?? events[0]?.report_dttm),
    [events, deal],
  );

  if (!id || events.length === 0) {
    if (id && cashflow) {
      return (
        <section className={styles.page}>
          <h1 className={styles.title}>Сделка {id}</h1>
          <p className={styles.hint}>Нет событий по этой сделке в загруженном кэшфлоу.</p>
        </section>
      );
    }
    return <NoDeal />;
  }

  const toggle = (t: PaymentType) =>
    setHidden((prev) => {
      const next = new Set(prev);
      if (next.has(t)) next.delete(t);
      else next.add(t);
      return next;
    });

  return (
    <section className={styles.page}>
      <div className={styles.toolbar}>
        <h1 className={styles.title}>
          Сделка <span className={ui.mono}>{id}</span>
        </h1>
        <button
          className={ui.buttonGhost}
          onClick={() => downloadText(cashflowToCsv(events), `cashflow_${id}.csv`)}
        >
          Скачать CSV сделки
        </button>
      </div>

      {deal && (
        <div className={ui.panel}>
          <h2 className={ui.panelTitle}>Параметры сделки</h2>
          <DealParams deal={deal} />
        </div>
      )}

      <div className={ui.panel}>
        <h2 className={ui.panelTitle}>Временная шкала</h2>
        <EChart option={option} height={460} />
      </div>

      <div className={styles.toolbar}>
        <h2 className={ui.panelTitle}>События</h2>
        <div className={dealStyles.filters}>
          {present.map((t) => (
            <label key={t} className={dealStyles.filter}>
              <input type="checkbox" checked={!hidden.has(t)} onChange={() => toggle(t)} />
              <TypeBadge type={t} />
            </label>
          ))}
        </div>
      </div>
      <VirtualTable
        data={visible}
        columns={columns}
        height={Math.min(560, 76 + visible.length * 34)}
        showFooter
        rowStyle={(e) =>
          e.payment_type === 'repricing' ? { background: 'var(--c-accent-light)' } : undefined
        }
        empty="Все типы событий скрыты фильтром"
      />
    </section>
  );
}
