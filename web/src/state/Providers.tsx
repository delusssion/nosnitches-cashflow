import type { ReactNode } from 'react';
import { CompareProvider } from './CompareContext';
import { DataProvider } from './DataContext';
import { ScenarioProvider } from './ScenarioContext';

export function Providers({ children }: { children: ReactNode }) {
  return (
    <DataProvider>
      <CompareProvider>
        <ScenarioProvider>{children}</ScenarioProvider>
      </CompareProvider>
    </DataProvider>
  );
}
