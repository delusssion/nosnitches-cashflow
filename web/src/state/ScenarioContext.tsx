import { createContext, useMemo, type ReactNode } from 'react';
import { useCashflowSide, type NamedBlob, type SideState } from './useCashflowSide';

export interface ScenarioState {
  /** «Было» — базовый прогон движка. */
  before: SideState;
  /** «Стало» — прогон с изменёнными параметрами, досрочкой или другим сценарием ставок. */
  after: SideState;
  loadBefore: (files: NamedBlob[]) => Promise<void>;
  loadAfter: (files: NamedBlob[]) => Promise<void>;
}

// eslint-disable-next-line react-refresh/only-export-components
export const ScenarioContext = createContext<ScenarioState | null>(null);

export function ScenarioProvider({ children }: { children: ReactNode }) {
  const [before, loadBefore] = useCashflowSide();
  const [after, loadAfter] = useCashflowSide();
  const value = useMemo(
    () => ({ before, after, loadBefore, loadAfter }),
    [before, after, loadBefore, loadAfter],
  );
  return <ScenarioContext.Provider value={value}>{children}</ScenarioContext.Provider>;
}
