import { createHashRouter, RouterProvider } from 'react-router';
import { routes } from './routes';

const router = createHashRouter(routes);

export function App() {
  return <RouterProvider router={router} />;
}
