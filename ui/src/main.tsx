import { QueryClientProvider } from '@tanstack/react-query';
import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import App from './App';
import './i18n';
import { onAppEvent } from './ipc/events';
import type { ServerScenario } from './ipc/mockServers';
import { theme, uiFont } from './ipc/schema';
import { applyAppearance, readMirror } from './state/appearance';
import { handleEvent } from './state/handleEvent';
import { hydrateRuns } from './state/hydrateRuns';
import { queryClient } from './state/queries';
import { installBrowserGuard } from './shell/browserGuard';
import { pushError } from './state/toasts';
import './styles/app.css';

// The last look, before anything renders, so the window never opens in the wrong theme. The settings
// confirm or correct it once they load (App).
applyAppearance(readMirror());

async function bootstrap() {
  // The Vite dev server (tauri dev, dev:mock) keeps the browser's menu and Inspect for developers.
  if (!import.meta.env.DEV) installBrowserGuard(document);
  if (import.meta.env.MODE === 'mock') {
    const { installMock, mockUpdateState } = await import('./ipc/mock');
    const { perlandScenario } = await import('./ipc/mockPerland');
    const { freeScenario } = await import('./ipc/mockFree');
    const params = new URLSearchParams(window.location.search);
    const themeParam = theme.safeParse(params.get('theme'));
    const fontParam = uiFont.safeParse(params.get('font'));
    installMock({
      firstRun: params.has('firstRun'),
      crash: params.has('crash'),
      release: params.has('release'),
      beta: params.has('beta'),
      update: mockUpdateState(params.get('update')),
      manual: params.has('manual'),
      noUpdate: params.has('noUpdate'),
      link: params.get('link') ?? undefined,
      perland: perlandScenario(params.get('perland')),
      free: freeScenario(params.get('free')),
      server: (params.get('server') as ServerScenario | null) ?? undefined,
      railExpanded: params.has('railExpanded'),
      iconShape: params.get('iconShape') === 'round' ? 'round' : undefined,
      theme: themeParam.success ? themeParam.data : undefined,
      font: fontParam.success ? fontParam.data : undefined,
      pictureBackground: !params.has('noPicture'),
    });
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
