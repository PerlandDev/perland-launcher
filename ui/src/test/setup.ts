import '@testing-library/jest-dom/vitest';
import { clearMocks } from '@tauri-apps/api/mocks';
import { cleanup } from '@testing-library/react';
import { afterEach } from 'vitest';
import i18n from '../i18n';
import { stopMockRuns } from '../ipc/mock';
import { resetBrowseSession } from '../features/browse/session';
import { resetServersSession } from '../features/servers-list/session';
import { resetActivity } from '../state/activity';
import { resetAppearance } from '../state/appearance';
import { resetContentProgress } from '../state/contentProgress';
import { resetImport } from '../state/importProgress';
import { manageLog } from '../state/manageLog';
import { resetRoute } from '../state/route';
import { runStore } from '../state/runStore';
import { clearToasts } from '../state/toasts';

// Dates (event times) read in the computer's zone; pin it so the tests don't depend on where they run, even with
// TZ exported in the shell. vite.config.ts also starts the workers in UTC.
(globalThis as unknown as { process: { env: Record<string, string | undefined> } }).process.env.TZ = 'UTC';

void i18n.changeLanguage('en');

// jsdom may lack <dialog> methods; the app relies on showModal/close.
if (typeof HTMLDialogElement !== 'undefined' && !HTMLDialogElement.prototype.showModal) {
  HTMLDialogElement.prototype.showModal = function showModal(this: HTMLDialogElement) {
    this.setAttribute('open', '');
  };
  HTMLDialogElement.prototype.close = function close(this: HTMLDialogElement) {
    this.removeAttribute('open');
    this.dispatchEvent(new Event('close'));
  };
}

if (typeof globalThis.ResizeObserver === 'undefined') {
  globalThis.ResizeObserver = class {
    observe() {}
    unobserve() {}
    disconnect() {}
  } as unknown as typeof ResizeObserver;
}
if (!Element.prototype.scrollTo) {
  Element.prototype.scrollTo = function scrollTo() {};
}

afterEach(async () => {
  cleanup();
  stopMockRuns();
  clearMocks();
  runStore.reset();
  resetRoute();
  clearToasts();
  resetActivity();
  resetBrowseSession();
  resetServersSession();
  resetContentProgress();
  resetImport();
  manageLog.reset();
  resetAppearance();
  await i18n.changeLanguage('en');
});
