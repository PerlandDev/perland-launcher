import '@testing-library/jest-dom/vitest';
import { clearMocks } from '@tauri-apps/api/mocks';
import { cleanup } from '@testing-library/react';
import { afterEach } from 'vitest';
import i18n from '../i18n';
import { stopMockRuns } from '../ipc/mock';
import { resetBrowseSession } from '../features/browse/session';
import { resetActivity } from '../state/activity';
import { resetContentProgress } from '../state/contentProgress';
import { clearImportProgress } from '../state/importProgress';
import { resetRoute } from '../state/route';
import { runStore } from '../state/runStore';
import { clearToasts } from '../state/toasts';

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
  resetContentProgress();
  clearImportProgress();
  await i18n.changeLanguage('en');
});
