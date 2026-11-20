import { createContext, useCallback, useMemo, useState, type ReactNode } from 'react';
import { useCashflowSide, type NamedBlob, type SideState } from './useCashflowSide';

export type { SideState } from './useCashflowSide';
export type CompareSide = 'engine' | 'golden';

export interface CompareState {
  engine: SideState;
  golden: SideState;
  tolerance: number;
  setTolerance: (t: number) => void;
  loadSide: (side: CompareSide, files: NamedBlob[]) => Promise<void>;
}

// eslint-disable-next-line react-refresh/only-export-components
export const CompareContext = createContext<CompareState | null>(null);

/** Состояние экрана сверки живёт выше роутера, чтобы не терялось при переходах между экранами. */
export function CompareProvider({ children }: { children: ReactNode }) {
  const [engine, loadEngine] = useCashflowSide();
  const [golden, loadGolden] = useCashflowSide();
  const [tolerance, setTolerance] = useState(0.01);

  const loadSide = useCallback(
    (side: CompareSide, files: NamedBlob[]) =>
      side === 'engine' ? loadEngine(files) : loadGolden(files),
    [loadEngine, loadGolden],
  );

  const value = useMemo(
    () => ({ engine, golden, tolerance, setTolerance, loadSide }),
    [engine, golden, tolerance, loadSide],
  );
  return <CompareContext.Provider value={value}>{children}</CompareContext.Provider>;
}
