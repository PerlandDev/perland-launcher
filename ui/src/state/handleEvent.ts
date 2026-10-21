import type { AppEvent, Settings } from '../ipc/schema';
import { openActivity } from './activity';
import { applyContentProgress } from './contentProgress';
import { applyImportProgress } from './importProgress';
import { takeDeepLink } from './deepLink';
import { isCrash } from './exit';
import { qk, queryClient } from './queries';
import { runStore } from './runStore';
import { pushError } from './toasts';

/** Everything the UI does with an event from Rust. */
export function handleEvent(event: AppEvent) {
  if (event.type === 'update') {
    queryClient.setQueryData(qk.update, event.state);
    return;
  }
  // A Browse install has its own progress; it never shows on Play.
  if (event.type === 'progress' && event.task.key === 'install_content') {
    applyContentProgress(event);
    return;
  }
  // So does an import from another launcher: it belongs to no instance yet.
  if (event.type === 'progress' && event.task.key === 'import_instance') {
    applyImportProgress(event);
    return;
  }
  runStore.apply(event);
  if (event.type === 'warning') pushError(event.error);
  // During first-run setup the link waits in Rust; the shell takes it once setup is done.
  if (event.type === 'deep_link' && queryClient.getQueryData<Settings>(qk.settings)?.firstRunDone) {
    void takeDeepLink();
  }
  if (event.type === 'run_state' && event.state === 'idle') {
    // A sync can change the game version, the pack's mods, resource packs and shaders, and its art.
    const id = event.instance;
    const keys = [
      qk.instances,
      qk.mods(id),
      qk.packStatus(id),
      qk.packContents(id),
      qk.instanceArt(id),
      qk.contentFiles(id, 'resource_pack'),
      qk.contentFiles(id, 'shader'),
      qk.serverStatus(id),
      qk.playtime,
      qk.continueTarget(id),
      qk.perlandStats,
    ];
    for (const queryKey of keys) {
      void queryClient.invalidateQueries({ queryKey });
    }
    const exit = event.exit;
    // Blocked files get their own dialog, not the activity drawer.
    if (exit && exit.manual.length === 0 && (exit.error || isCrash(exit))) openActivity(id);
  }
}
