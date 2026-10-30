import assert from 'node:assert/strict';
import { test } from 'node:test';
import { DEAL_COLUMNS, makeDeal } from './deal.ts';
import { Rng } from './random.ts';

const REPORT = '2022-08-31';
const sample = (n: number, seed = 42) => {
  const rng = new Rng(seed);
  return Array.from({ length: n }, (_, i) => makeDeal(rng, i, REPORT));
};

test('детерминизм: один seed — одинаковые сделки, другой — другие', () => {
  assert.deepEqual(sample(200), sample(200));
  assert.notDeepEqual(sample(200, 1), sample(200, 2));
});

test('распределение близко к заданному', () => {
  const deals = sample(50_000);
  const share = (f: (d: (typeof deals)[number]) => boolean) =>
    deals.filter(f).length / deals.length;
  const near = (v: number, target: number) =>
    assert.ok(Math.abs(v - target) < 0.01, `${v} ≉ ${target}`);
  near(
    share((d) => d.discriminator === 'ANN'),
    0.35,
  );
  near(
    share((d) => d.discriminator === 'RGM'),
    0.3,
  );
  near(
    share((d) => d.discriminator === 'SIMPLE'),
    0.35,
  );
  near(
    share((d) => d.rate_type_cd === 'FLOAT'),
    0.3,
  );
  near(
    share((d) => d.sub_rate_type_cd === 'FLOAT'),
    0.1,
  );
  near(
    share((d) => Number(d.current_principal_amt) < 0),
    0.1,
  );
});

test('поля в границах контракта', () => {
  for (const d of sample(20_000)) {
    const amt = Math.abs(Number(d.start_principal_amt));
    assert.ok(amt >= 50_000 && amt <= 20_000_000);
    assert.ok(String(d.maturity_dttm) > REPORT, 'сделка не погашена на дату расчёта');
    assert.ok(String(d.deal_dttm) <= REPORT);
    const day = Number(d.interest_payment_rule_day_num);
    assert.ok(day === -1 || (day >= 1 && day <= 28));
    assert.ok(['1M', '3M', 'ENDDATE'].includes(String(d.interest_payment_rule_period_cd)));
    assert.ok(['ACT/365', 'ACT/ACT ISDA'].includes(String(d.day_convention_cd)));
    if (d.rate_type_cd === 'FLOAT') assert.equal(d.base_indicator, 'CBKEY1M');
  }
  assert.equal(DEAL_COLUMNS.length, 48);
});
