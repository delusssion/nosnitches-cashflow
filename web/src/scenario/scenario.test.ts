import after from '../../../testdata/samples/scenario/cashflow_after.csv?raw';
import before from '../../../testdata/samples/scenario/cashflow_before.csv?raw';
import { parseCsvText } from '../data/parseCsv';
import { balancePath, diffRows, scenarioSummary } from './scenario';

const b = parseCsvText(before, 'cashflow').rows;
const a = parseCsvText(after, 'cashflow').rows;

describe('сценарий «было / стало» (вывод движка)', () => {
  const summary = scenarioSummary(b, a);
  const deal = (id: string) => summary.find((d) => d.dealId === id)!;

  it('все три сделки изменились', () => {
    expect(summary.map((d) => d.dealId)).toEqual(['7', '22', '25']);
    expect(summary.every((d) => d.changedRows > 0)).toBe(true);
  });

  it('досрочка по сделке 22 экономит проценты и появляется новой строкой', () => {
    expect(deal('22').dInterest).toBeCloseTo(72117.54 - 83551.0, 1);
    expect(deal('22').closeShiftDays).toBe(0); // закрытие остаётся в дату погашения (B13)
    expect(deal('22').after!.prepayment).toBe(250000);
    const rows = diffRows(
      b.filter((e) => e.deal_id === '22'),
      a.filter((e) => e.deal_id === '22'),
    );
    const prepay = rows.find((r) => r.type === 'prepayment')!;
    expect(prepay).toMatchObject({ status: 'added', date: '2022-11-20', before: null });
  });

  it('рост ставки по сделке 7 и снятие cap по сделке 25 увеличивают проценты', () => {
    expect(deal('7').dInterest).toBeCloseTo(14777.38 - 11059.76, 1);
    expect(deal('25').dInterest).toBeCloseTo(27246.59 - 20958.92, 1);
    expect(deal('7').closeShiftDays).toBe(0);
  });

  it('тело в сумме не меняется: досрочка перераспределяет его по датам', () => {
    // суммы в CSV округлены до копеек: каждая строка тела может дать до полкопейки
    const d22 = deal('22');
    const rows = d22.after!.bodyPayments;
    expect(Math.abs(d22.after!.principal - d22.before!.principal)).toBeLessThanOrEqual(
      0.005 * rows + 1e-9,
    );
  });

  it('путь остатка заканчивается нулём', () => {
    const path = balancePath(
      a.filter((e) => e.deal_id === '22'),
      '2022-08-31',
    );
    expect(path[0]).toEqual(['2022-08-31', 1000000]);
    expect(path.at(-1)![1]).toBe(0);
  });
});
