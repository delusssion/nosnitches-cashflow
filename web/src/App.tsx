import { createHashRouter, RouterProvider } from 'react-router';
import { routes } from './routes';
import { DataProvider } from './state/DataContext';

const router = createHashRouter(routes);

export function App() {
  return (
    <DataProvider>
      <RouterProvider router={router} />
    </DataProvider>
  );
}
