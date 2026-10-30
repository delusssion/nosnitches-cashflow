import type { Deal } from '../data/types';
import { fmtDate, fmtMoney, fmtRate } from '../format';
import styles from './Deal.module.css';

type Kind = 'date' | 'money' | 'rate' | 'text';

// Порядок и подписи полей карточки; остальные поля сделки показываются под своим именем.
const FIELDS: [keyof Deal, string, Kind][] = [
  ['discriminator', 'Тип погашения', 'text'],
  ['product', 'Продукт', 'text'],
  ['currency_id', 'Валюта', 'text'],
  ['start_principal_amt', 'Сумма выдачи', 'money'],
  ['current_principal_amt', 'Остаток на дату расчёта', 'money'],
  ['report_dttm', 'Дата расчёта', 'date'],
  ['deal_dttm', 'Дата сделки', 'date'],
  ['maturity_dttm', 'Дата погашения', 'date'],
  ['rate_type_cd', 'Тип ставки', 'text'],
  ['interest_rate', 'Ставка / маржа', 'rate'],
  ['base_indicator', 'Индикатор', 'text'],
  ['base_indicator_rate', 'Значение индикатора', 'rate'],
  ['indicator_lag_cd', 'Лаг индикатора', 'text'],
  ['day_convention_cd', 'Конвенция дней', 'text'],
  ['interest_payment_rule_start_dttm', 'Проценты: начало', 'date'],
  ['interest_payment_rule_period_cd', 'Проценты: период', 'text'],
  ['interest_payment_rule_day_num', 'Проценты: число', 'text'],
  ['regular_payment_rule_start_dttm', 'Тело: начало', 'date'],
  ['regular_payment_rule_period_cd', 'Тело: период', 'text'],
  ['regular_payment_rule_day_num', 'Тело: число', 'text'],
  ['repricing_rule_start_dttm', 'Репрайсинг: начало', 'date'],
  ['repricing_rule_period_cd', 'Репрайсинг: период', 'text'],
  ['repricing_rule_day_num', 'Репрайсинг: число', 'text'],
  ['repricing_dttm_schedule', 'Даты репрайсинга', 'text'],
  ['cap_limit_interest_rate', 'Cap', 'rate'],
  ['floor_limit_interest_rate', 'Floor', 'rate'],
  ['accrued_interest', 'Накопленные проценты', 'money'],
  ['sub_rate_type_cd', 'Субсидия: тип ставки', 'text'],
  ['sub_interest_rate', 'Субсидия: ставка', 'rate'],
  ['sub_float_indicator', 'Субсидия: индикатор', 'text'],
  ['sub_coeff_index', 'Субсидия: коэффициент', 'text'],
  ['sub_payment_rule_period_cd', 'Субсидия: период', 'text'],
  ['sub_payment_rule_day_num', 'Субсидия: число', 'text'],
  ['sub_cap_limit_interest_rate', 'Субсидия: cap', 'rate'],
  ['sub_floor_limit_interest_rate', 'Субсидия: floor', 'rate'],
];
const KNOWN = new Set<string>(FIELDS.map(([k]) => k));

function format(v: unknown, kind: Kind): string {
  if (kind === 'date') return fmtDate(v as string);
  if (kind === 'money') return fmtMoney(v as number);
  if (kind === 'rate') return fmtRate(v as number);
  return String(v);
}

export function DealParams({ deal }: { deal: Deal }) {
  const rows: [string, string][] = [];
  for (const [key, label, kind] of FIELDS) {
    if (deal[key] !== null && deal[key] !== '') rows.push([label, format(deal[key], kind)]);
  }
  for (const [key, v] of Object.entries(deal)) {
    if (!KNOWN.has(key) && key !== 'deal_id' && v !== null && v !== '') rows.push([key, String(v)]);
  }
  return (
    <dl className={styles.params}>
      {rows.map(([label, value]) => (
        <div key={label} className={styles.param}>
          <dt>{label}</dt>
          <dd title={value}>{value}</dd>
        </div>
      ))}
    </dl>
  );
}
