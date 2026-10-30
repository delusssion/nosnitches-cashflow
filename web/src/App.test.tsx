import { render, screen } from '@testing-library/react';
import { createMemoryRouter, RouterProvider } from 'react-router';
import { routes } from './routes';
import { Providers } from './state/Providers';

function renderAt(path: string) {
  const router = createMemoryRouter(routes, { initialEntries: [path] });
  render(
    <Providers>
      <RouterProvider router={router} />
    </Providers>,
  );
}

describe('Layout', () => {
  it('показывает шапку и навигацию', () => {
    renderAt('/');
    expect(screen.getByText('nosnitches')).toBeInTheDocument();
    for (const label of ['Портфель', 'Сделка', 'Сверка']) {
      expect(screen.getByRole('link', { name: label })).toBeInTheDocument();
    }
  });

  it('открывает экран сверки по маршруту', () => {
    renderAt('/compare');
    expect(screen.getByRole('heading', { name: 'Сверка с эталоном' })).toBeInTheDocument();
  });

  it('на портфеле есть зоны загрузки и кнопка демо', () => {
    renderAt('/');
    expect(screen.getByText('Перетащите кэшфлоу (CSV)')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Загрузить демо' })).toBeInTheDocument();
  });
});
