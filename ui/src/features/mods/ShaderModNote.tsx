import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { isLauncherError } from '../../ipc/errors';
import type { Instance, ShaderMod } from '../../ipc/schema';
import { useBrowseVersions, useShaderSupport } from '../../state/queries';
import { navigate } from '../../state/route';
import { Button } from '../../ui/Button';
import { ErrorCard } from '../../ui/ErrorCard';
import { Download, Package } from '../../ui/Icon';
import { fitFor } from '../browse/compat';
import { InstallDialog } from '../browse/InstallDialog';
import { useInstallFlow } from '../browse/useInstallFlow';

/** Proper names, the same in every language. The key is the mod's Modrinth slug. */
const NAMES: Record<ShaderMod, string> = { iris: 'Iris', oculus: 'Oculus' };

/**
 * Shader packs only show in game through a shader mod. On a modded instance without one (the pack's
 * own mods and the player's count), a note says so and adds the mod through Browse's install flow.
 * Nothing on vanilla, where the loader note speaks instead, and nothing once the mod is there.
 */
export function ShaderModNote({ instance }: { instance: Instance }) {
  const support = useShaderSupport(instance.id, instance.game.loader !== null);
  const wanted = support.data && !support.data.installed ? support.data.wanted : null;
  return wanted ? <MissingShaderMod instance={instance} mod={wanted} /> : null;
}

function MissingShaderMod({ instance, mod }: { instance: Instance; mod: ShaderMod }) {
  const { t } = useTranslation();
  const name = NAMES[mod];
  const versions = useBrowseVersions(mod);
  const [failure, setFailure] = useState<{ error: unknown } | null>(null);
  // The core found no version after all (the list was stale): the same plain sentence as up front.
  const [refused, setRefused] = useState(false);
  const flow = useInstallFlow({
    onError: (error) => {
      if (isLauncherError(error) && error.code === 'content_incompatible') setRefused(true);
      else setFailure({ error });
    },
  });
  // Offline or rate limited, the list can't say; the button tries, and a failure shows below.
  if (versions.isPending) return null;
  const noVersion = refused || (versions.isSuccess && !fitFor(versions.data, 'mod', instance).ok);
  // Like any mod the player adds, it can go in while the game runs and applies the next time they press Play.
  const waiting = flow.busy;
  const add = () => {
    if (waiting) return;
    setFailure(null);
    void flow.start({ project: mod, version: null, target: { kind: 'instance', id: instance.id } });
  };
  return (
    <>
      {/* The first line sits on the button's middle (a 40px key, 22.5px lines), and its lamp with it; more lines run below. */}
      <div className="note-plate before:mt-[15.75px]">
        <div className="min-w-0 flex-1 pt-[8.75px]">
          <p>{noVersion ? t('shaderMod.noVersion', { mod: name }) : t('shaderMod.missing', { mod: name })}</p>
        </div>
        {noVersion ? (
          <Button
            variant="ghost"
            icon={<Package />}
            className="shrink-0"
            onClick={() => navigate({ name: 'browse', kind: 'mod', instance: instance.id, project: mod })}
          >
            {t('shaderMod.open', { mod: name })}
          </Button>
        ) : (
          <Button icon={<Download />} className="shrink-0" aria-disabled={waiting || undefined} onClick={add}>
            {flow.busy ? t('shaderMod.adding', { mod: name }) : t('shaderMod.add', { mod: name })}
          </Button>
        )}
      </div>
      {failure && <ErrorCard error={failure.error} />}
      {flow.confirming && (
        <InstallDialog view={flow.confirming} busy={flow.busy} onConfirm={() => void flow.install(flow.confirming!)} onClose={flow.cancel} />
      )}
    </>
  );
}
