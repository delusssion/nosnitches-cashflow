import { useContext } from 'react';
import { ScenarioContext, type ScenarioState } from './ScenarioContext';

export function useScenario(): ScenarioState {
  const ctx = useContext(ScenarioContext);
  if (!ctx) throw new Error('useScenario вне ScenarioProvider');
  return ctx;
}
