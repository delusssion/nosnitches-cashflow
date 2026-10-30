import { useContext } from 'react';
import { DataContext, type DataState } from './DataContext';

export function useData(): DataState {
  const ctx = useContext(DataContext);
  if (!ctx) throw new Error('useData вне DataProvider');
  return ctx;
}
