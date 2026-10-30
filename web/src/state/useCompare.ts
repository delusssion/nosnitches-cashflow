import { useContext } from 'react';
import { CompareContext, type CompareState } from './CompareContext';

export function useCompare(): CompareState {
  const ctx = useContext(CompareContext);
  if (!ctx) throw new Error('useCompare вне CompareProvider');
  return ctx;
}
