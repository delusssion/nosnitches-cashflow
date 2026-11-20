import keyRate from '../../../testdata/rates/key_rate.csv?raw';
import { changes, forecastH3, knownAt, parseRates, valueAt } from './rates';

describe('ставки', () => {
  const { series, errors, rowCount } = parseRates(keyRate);
  const kr = series.get('CBKEY1M')!;

  it('история КС ЦБ из testdata читается без ошибок', () => {
    expect(errors).toEqual([]);
    expect(rowCount).toBe(kr.length);
    expect(kr[0]).toEqual(['2020-01-03', 0.0625]);
  });

  it('значение на дату — ступенькой', () => {
    expect(valueAt(kr, '2022-08-31')).toBe(0.08);
    expect(valueAt(kr, '2022-07-15')).toBe(0.095);
    expect(valueAt(kr, '2019-12-31')).toBeNull();
  });

  it('прогноз H3: значение на дату расчёта плюс сдвиг', () => {
    const f = forecastH3(kr, '2022-08-31', 200, '2025-12-31');
    expect(f).toEqual([
      ['2022-08-31', 0.1],
      ['2025-12-31', 0.1],
    ]);
    expect(knownAt(kr, '2022-08-31').at(-1)![0] <= '2022-08-31').toBe(true);
  });

  it('ступеньки — только даты изменения', () => {
    const c = changes(knownAt(kr, '2022-08-31'));
    expect(c.map(([d, v]) => `${d}:${v}`)).toContain('2022-07-25:0.08');
    expect(c.length).toBeLessThan(40);
  });

  it('формат кривых команды 2 со сценариями', () => {
    const csv =
      'rate_date,indicator_code,rate_value,scenario\n2022-09-01,CBKEY1M,0.08,base\n2022-09-01,CBKEY1M,0.1,stress\n2022-10-01,CBKEY1M,0.085,base';
    const r = parseRates(csv);
    expect(r.errors).toEqual([]);
    expect([...r.series.keys()]).toEqual(['CBKEY1M · base', 'CBKEY1M · stress']);
    expect(r.series.get('CBKEY1M · base')).toHaveLength(2);
  });

  it('ошибки: плохая дата, плохое число, нет колонок', () => {
    const r = parseRates('indicator,date,rate\nCBKEY1M,31.08.2022,0.08\nCBKEY1M,2022-08-31,8%');
    expect(r.errors.map((e) => [e.row, e.field])).toEqual([
      [1, 'date'],
      [2, 'rate'],
    ]);
    expect(parseRates('a,b\n1,2').errors[0]!.row).toBe(0);
  });
});
