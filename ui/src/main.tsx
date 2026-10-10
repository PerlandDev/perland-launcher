import { QueryClientProvider } from '@tanstack/react-query';
import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import App from './App';
import './i18n';
import { onAppEvent } from './ipc/events';
import { handleEvent } from './state/handleEvent';
import { hydrateRuns } from './state/hydrateRuns';
import { queryClient } from './state/queries';
import { pushError } from './state/toasts';
import './styles/app.css';

async function bootstrap() {
  if (import.meta.env.MODE === 'mock') {
    const { installMock } = await import('./ipc/mock');
    const params = new URLSearchParams(window.location.search);
    installMock({ firstRun: params.has('firstRun'), crash: params.has('crash'), release: params.has('release') });
  }
  // Whatever fails here, the window still opens; the failure shows up as a toast.
  try {
    await onAppEvent(handleEvent);
  } catch (error) {
    pushError(error);
  }
  await hydrateRuns();

  createRoot(document.getElementById('root')!).render(
    <StrictMode>
      <QueryClientProvider client={queryClient}>
        <App />
      </QueryClientProvider>
    </StrictMode>,
  );
}

void bootstrap();
