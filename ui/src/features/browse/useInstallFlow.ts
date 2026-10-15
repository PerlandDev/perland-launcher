import { useQueryClient } from '@tanstack/react-query';
import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { commands } from '../../ipc/commands';
import type { BrowsePlanRequest, InstallPlanView, InstallResult } from '../../ipc/schema';
import { beginContentInstall, clearContentProgress } from '../../state/contentProgress';
import { qk } from '../../state/queries';
import { navigate } from '../../state/route';
import { runStore } from '../../state/runStore';
import { pushError, pushNotice } from '../../state/toasts';

/** Ask first when the install adds more than the one thing, skips something, or makes an instance. */
export const needsConfirm = (view: InstallPlanView) =>
  view.items.length > 1 || view.skipped.length > 0 || view.target.kind === 'new';

export function useInstallFlow() {
  const { t } = useTranslation();
  const qc = useQueryClient();
  const [confirming, setConfirming] = useState<InstallPlanView | null>(null);
  const [busy, setBusy] = useState(false);

  const finish = (view: InstallPlanView, result: InstallResult) => {
    const id = result.instance.id;
    clearContentProgress(id);
    const keys = [
      qk.instances,
      qk.browseInstalled(id),
      qk.mods(id),
      qk.packContents(id),
      qk.contentFiles(id, 'resource_pack'),
      qk.contentFiles(id, 'shader'),
    ];
    for (const queryKey of keys) {
      void qc.invalidateQueries({ queryKey });
    }
    const first = view.items[0];
    const name = first?.title ?? '';
    const more = view.items.length - 1;
    const instance = result.instance.name;
    const state = runStore.run(id).state;
    const playing = state === 'preparing' || state === 'running' || state === 'stopping';
    pushNotice({
      title: more > 0 ? t('install.addedMore', { count: more, name, instance }) : t('install.added', { name, instance }),
      body: playing
        ? t('install.restart')
        : first?.kind === 'resource_pack'
          ? t('install.turnOnResourcePack')
          : first?.kind === 'shader'
            ? t('install.turnOnShader')
            : undefined,
      action: { label: t('install.open'), run: () => navigate({ name: 'home', id }) },
    });
    if (view.target.kind === 'new') navigate({ name: 'home', id });
  };

  const install = async (view: InstallPlanView) => {
    setBusy(true);
    const ended = beginContentInstall();
    let result: InstallResult;
    try {
      result = await commands.browseInstall(view.planId);
    } catch (error) {
      // A failed install must not leave its meter lit. A new instance has no id the UI knows yet.
      if (view.target.kind === 'instance') clearContentProgress(view.target.id);
      // A New install makes its instance first and keeps it when the install fails, so the list has changed.
      else void qc.invalidateQueries({ queryKey: qk.instances });
      pushError(error);
      return;
    } finally {
      ended();
      setConfirming(null);
      setBusy(false);
    }
    // The install worked; a slip in the notice or navigation is not a failed install.
    try {
      finish(view, result);
    } catch (error) {
      console.error('Could not show the install result', error);
    }
  };

  const start = async (request: BrowsePlanRequest) => {
    setBusy(true);
    let view: InstallPlanView;
    try {
      view = await commands.browsePlan(request);
    } catch (error) {
      pushError(error);
      setBusy(false);
      return;
    }
    if (needsConfirm(view)) {
      setConfirming(view);
      setBusy(false);
      return;
    }
    await install(view);
  };

  return { busy, confirming, start, install, cancel: () => setConfirming(null) };
}
