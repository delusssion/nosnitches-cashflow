// Типы по docs/CONTRACT.md: формат 1 (сделки) и формат 3 (кэшфлоу).
// Даты храним строками YYYY-MM-DD: они сравниваются лексикографически и дёшевы на сотнях тысяч строк.

/** Дата в формате YYYY-MM-DD. */
export type IsoDate = string;

// Вместо TS enum — const-объекты (erasableSyntaxOnly в tsconfig).
export const PaymentType = {
  Interest: 'interest_payment',
  Principal: 'principal_payment',
  Close: 'close_payment',
  Repricing: 'repricing',
  Sub: 'sub_payment',
  SubRepricing: 'sub_repricing',
  Prepayment: 'prepayment',
} as const;
export type PaymentType = (typeof PaymentType)[keyof typeof PaymentType];

export const Stream = {
  Client: 'client',
  Sub: 'sub',
} as const;
export type Stream = (typeof Stream)[keyof typeof Stream];

export const Discriminator = {
  Ann: 'ANN',
  Rgm: 'RGM',
  Simple: 'SIMPLE',
} as const;
export type Discriminator = (typeof Discriminator)[keyof typeof Discriminator];

/** Порядок событий в один день по контракту. */
export const EVENT_DAY_ORDER: Record<PaymentType, number> = {
  repricing: 0,
  sub_repricing: 0,
  interest_payment: 1,
  sub_payment: 2,
  principal_payment: 3,
  close_payment: 4,
  prepayment: 5,
};

export interface CashflowEvent {
  report_dttm: IsoDate;
  deal_id: string;
  event_order: number;
  payment_dttm: IsoDate;
  payment_type: PaymentType;
  stream: Stream;
  amount: number;
  principal_before: number | null;
  principal_after: number | null;
  rate: number | null;
  base_rate: number | null;
  accrual_start_dttm: IsoDate | null;
  accrual_end_dttm: IsoDate | null;
  accrual_days: number | null;
  year_frac: number | null;
  discriminator: Discriminator | null;
}

export interface Deal {
  report_dttm: IsoDate;
  discriminator: Discriminator;
  deal_id: string;
  currency_id: string | null;
  start_principal_amt: number | null;
  current_principal_amt: number;
  deal_dttm: IsoDate | null;
  maturity_dttm: IsoDate;
  rate_type_cd: 'FIXED' | 'FLOAT';
  interest_rate: number | null;
  base_indicator_rate: number | null;
  base_indicator: string | null;
  interest_payment_rule_start_dttm: IsoDate | null;
  interest_payment_rule_period_cd: string | null;
  interest_payment_rule_day_num: number | null;
  day_convention_cd: string | null;
  regular_payment_rule_start_dttm: IsoDate | null;
  regular_payment_rule_period_cd: string | null;
  regular_payment_rule_day_num: number | null;
  repricing_rule_start_dttm: IsoDate | null;
  repricing_rule_period_cd: string | null;
  repricing_rule_day_num: number | null;
  accrued_interest: number | null;
  product: string | null;
  behavior_model_cd: string | null;
  interest_payment: string | null;
  principal_payment: string | null;
  repricing_dttm_schedule: string | null;
  repricing_dttm_rate_schedule: string | null;
  repricing_dttm_rate_schedule_fixed: string | null;
  cap_limit_interest_rate: number | null;
  floor_limit_interest_rate: number | null;
  sub_rate_type_cd: string | null;
  sub_interest_rate: number | null;
  sub_float_indicator: string | null;
  sub_payment_rule_period_cd: string | null;
  sub_payment_rule_day_num: number | null;
  sub_payment_rule_start_dttm: IsoDate | null;
  sub_repricing_rule_start_dttm: IsoDate | null;
  sub_repricing_rule_period_cd: string | null;
  sub_repricing_rule_day_num: number | null;
  sub_cap_limit_interest_rate: number | null;
  sub_floor_limit_interest_rate: number | null;
  sub_coeff_index: number | null;
  deal_report_dttm: IsoDate | null;
  sub_repricing_rule_first_event_flg: string | null;
  indicator_lag_cd: string | null;
  sub_indicator_lag_cd: string | null;
}

export interface ValidationError {
  /** Номер строки данных, с 1 (заголовок не считается). */
  row: number;
  field: string;
  message: string;
}

export type CsvKind = 'cashflow' | 'deals';

export interface RowTypes {
  cashflow: CashflowEvent;
  deals: Deal;
}
