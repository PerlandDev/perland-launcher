import type { ReactElement, ReactNode } from 'react';
import { useTranslation } from 'react-i18next';
import { commands } from '../../ipc/commands';
import type { CrashFix, Instance, ProjectDetail } from '../../ipc/schema';
import { suggestMemory } from '../../state/exit';
import { useAppInfo, useModMutations, useUpdateInstance } from '../../state/queries';
import { navigate } from '../../state/route';
import { runStore } from '../../state/runStore';
import { pushError, pushNotice } from '../../state/toasts';
import { Button } from '../../ui/Button';
import { formatGb } from '../../ui/format';
import { DRIVER_PAGES } from './driverPages';
import { Download, ExternalLink, MemoryStick, MinusBox, Reload, Search, SettingsCog } from '../../ui/Icon';

type Variant = 'secondary' | 'ghost';

interface FixProps {
  instance: Instance;
  variant: Variant;
}

/** The fixes the launcher can apply for a crash, in its order: the first raised, the rest ghost. Never lit: Play stays
 *  the one amber button, and pressing it again is the next step. A fix that can't show (a mod not on Modrinth) is skipped. */
export function CrashFixes({
  instance,
  fixes,
  found,
  children,
}: {
  instance: Instance;
  fixes: CrashFix[];
  /** Modrinth's mods for the Find fixes (`useModrinthMods`). */
  found: Map<string, ProjectDetail>;
  children?: ReactNode;
}) {
  // A Find shows only once Modrinth has the project as a mod, so the primary is the first fix that shows.
  const shown = fixes.filter((fix) => fix.kind !== 'find_mod' || found.has(fix.project));
  return (
    <div className="flex flex-wrap gap-3">
      {shown.map((fix, i) => (
        <Fix
          key={`${fix.kind}-${i}`}
          fix={fix}
          project={fix.kind === 'find_mod' ? found.get(fix.project) : undefined}
          instance={instance}
          variant={i === 0 ? 'secondary' : 'ghost'}
        />
      ))}
      {children}
    </div>
  );
}

function Fix({ fix, project, ...props }: FixProps & { fix: CrashFix; project?: ProjectDetail }): ReactElement | null {
  switch (fix.kind) {
    case 'more_memory':
      return <MoreMemory {...props} />;
    case 'recommended_java':
      return <RecommendedJava {...props} />;
    case 'java_settings':
      return <OpenSettings {...props} label="activity.fixJavaSettings" />;
    case 'find_mod':
      return project ? <FindMod {...props} project={project} /> : null;
    case 'disable_mod':
      return <DisableMod {...props} file={fix.file} name={fix.name} />;
    case 'repair':
      return <Repair {...props} />;
    case 'update_drivers':
      return <UpdateDrivers {...props} url={DRIVER_PAGES[fix.vendor]} />;
  }
}

const done = (instance: Instance, title: string) => {
  runStore.dismissExit(instance.id);
  pushNotice({ title });
};

function MoreMemory({ instance, variant }: FixProps) {
  const { t, i18n } = useTranslation();
  const appInfo = useAppInfo();
  const update = useUpdateInstance();
  if (!appInfo.data) return null;
  const raised = suggestMemory(instance.java.memoryMb, appInfo.data.systemMemoryMb);
  // No room left on this computer: the settings say how much there is.
  if (raised === null) return <OpenSettings instance={instance} variant={variant} label="activity.fixMemorySettings" icon={<MemoryStick />} />;
  const gb = formatGb(raised, i18n.language);
  return (
    <Button
      variant={variant}
      icon={<MemoryStick />}
      disabled={update.isPending}
      onClick={() =>
        update.mutate(
          { ...instance, java: { ...instance.java, memoryMb: raised } },
          { onSuccess: () => done(instance, t('activity.memoryRaised', { gb })) },
        )
      }
    >
      {t('activity.raiseMemory', { gb })}
    </Button>
  );
}

function RecommendedJava({ instance, variant }: FixProps) {
  const { t } = useTranslation();
  const update = useUpdateInstance();
  return (
    <Button
      variant={variant}
      icon={<Reload />}
      disabled={update.isPending}
      onClick={() =>
        update.mutate({ ...instance, java: { ...instance.java, path: null } }, { onSuccess: () => done(instance, t('activity.javaReset')) })
      }
    >
      {t('activity.fixRecommendedJava')}
    </Button>
  );
}

function OpenSettings({ instance, variant, label, icon = <SettingsCog /> }: FixProps & { label: string; icon?: ReactElement }) {
  const { t } = useTranslation();
  return (
    <Button variant={variant} icon={icon} onClick={() => navigate({ name: 'instanceSettings', id: instance.id })}>
      {t(label)}
    </Button>
  );
}

/** Opens the project's page for this instance, which says whether a version fits and installs it the usual way. */
function FindMod({ instance, variant, project }: FixProps & { project: ProjectDetail }) {
  const { t } = useTranslation();
  const { slug, title } = project;
  return (
    <Button variant={variant} icon={<Search />} onClick={() => navigate({ name: 'browse', kind: 'mod', instance: instance.id, project: slug })}>
      {t('activity.fixFindMod', { name: title })}
    </Button>
  );
}

function DisableMod({ instance, variant, file, name }: FixProps & { file: string; name: string }) {
  const { t } = useTranslation();
  const { toggle } = useModMutations(instance.id);
  return (
    <Button
      variant={variant}
      icon={<MinusBox />}
      disabled={toggle.isPending}
      onClick={() => toggle.mutate({ file, enabled: false }, { onSuccess: () => done(instance, t('activity.modTurnedOff', { name })) })}
    >
      {t('activity.fixDisableMod', { name })}
    </Button>
  );
}

function Repair({ instance, variant }: FixProps) {
  const { t } = useTranslation();
  return (
    <Button
      variant={variant}
      icon={<Download />}
      onClick={() => {
        runStore.dismissExit(instance.id);
        void commands.playRepair(instance.id).catch(pushError);
      }}
    >
      {t('activity.fixRepair')}
    </Button>
  );
}

function UpdateDrivers({ variant, url }: FixProps & { url: string }) {
  const { t } = useTranslation();
  return (
    <Button variant={variant} icon={<ExternalLink />} onClick={() => void commands.openLink(url).catch(pushError)}>
      {t('activity.fixUpdateDrivers')}
    </Button>
  );
}
