import type { PaymentType } from '../data/types';

export const PAYMENT_TYPE_INFO: Record<PaymentType, { label: string; color: string; bg: string }> =
  {
    interest_payment: { label: 'проценты', color: '#B4541A', bg: '#F7E6DA' },
    principal_payment: { label: 'тело', color: '#3E5BA8', bg: '#E1E7F5' },
    close_payment: { label: 'закрытие', color: '#2A3F78', bg: '#D3DBF0' },
    repricing: { label: 'репрайсинг', color: '#0E6B57', bg: '#D5EBE4' },
    sub_payment: { label: 'субсидия', color: '#8A4F9E', bg: '#EEE1F2' },
    sub_repricing: { label: 'репрайсинг субсидии', color: '#6B3A7C', bg: '#E6D6EC' },
    prepayment: { label: 'досрочное', color: '#55665F', bg: '#E4EAE8' },
  };

export const PAYMENT_TYPES = Object.keys(PAYMENT_TYPE_INFO) as PaymentType[];
