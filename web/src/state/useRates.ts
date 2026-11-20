import { useContext } from 'react';
import { RatesContext, type RatesState } from './RatesContext';

export function useRates(): RatesState {
  const ctx = useContext(RatesContext);
  if (!ctx) throw new Error('useRates вне RatesProvider');
  return ctx;
}
