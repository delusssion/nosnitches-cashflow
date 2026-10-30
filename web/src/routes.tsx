import type { RouteObject } from 'react-router';
import { Layout } from './layout/Layout';
import { ComparePage } from './pages/ComparePage';
import { DealPage } from './pages/DealPage';
import { PortfolioPage } from './pages/PortfolioPage';

export const routes: RouteObject[] = [
  {
    path: '/',
    element: <Layout />,
    children: [
      { index: true, element: <PortfolioPage /> },
      { path: 'deal', element: <DealPage /> },
      { path: 'deal/:id', element: <DealPage /> },
      { path: 'compare', element: <ComparePage /> },
    ],
  },
];
