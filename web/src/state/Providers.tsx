import type { ReactNode } from 'react';
import { CompareProvider } from './CompareContext';
import { DataProvider } from './DataContext';
import { RatesProvider } from './RatesContext';
import { ScenarioProvider } from './ScenarioContext';

export function Providers({ children }: { children: ReactNode }) {
  return (
    <DataProvider>
      <CompareProvider>
        <ScenarioProvider>
          <RatesProvider>{children}</RatesProvider>
        </ScenarioProvider>
      </CompareProvider>
    </DataProvider>
  );
}
