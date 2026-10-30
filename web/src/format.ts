const money = new Intl.NumberFormat('ru-RU', {
  minimumFractionDigits: 2,
  maximumFractionDigits: 2,
});
const integer = new Intl.NumberFormat('ru-RU', { maximumFractionDigits: 0 });
const percent = new Intl.NumberFormat('ru-RU', {
  style: 'percent',
  minimumFractionDigits: 2,
  maximumFractionDigits: 3,
});

/** 1234567.891 → «1 234 567,89». */
export const fmtMoney = (v: number | null | undefined) => (v == null ? '—' : money.format(v));

export const fmtInt = (v: number | null | undefined) => (v == null ? '—' : integer.format(v));

/** 0.075 → «7,50 %». */
export const fmtRate = (v: number | null | undefined) => (v == null ? '—' : percent.format(v));

/** 2022-08-31 → 31.08.2022. */
export const fmtDate = (v: string | null | undefined) => {
  if (!v) return '—';
  const [y, m, d] = v.split('-');
  return `${d}.${m}.${y}`;
};
