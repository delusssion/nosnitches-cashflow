import type { ReactNode } from 'react';
import { CompareProvider } from './CompareContext';
import { DataProvider } from './DataContext';

export function Providers({ children }: { children: ReactNode }) {
  return (
    <DataProvider>
      <CompareProvider>{children}</CompareProvider>
    </DataProvider>
  );
}
