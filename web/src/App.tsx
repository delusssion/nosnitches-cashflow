import { createHashRouter, RouterProvider } from 'react-router';
import { routes } from './routes';
import { Providers } from './state/Providers';

const router = createHashRouter(routes);

export function App() {
  return (
    <Providers>
      <RouterProvider router={router} />
    </Providers>
  );
}
