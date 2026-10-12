import { QueryClient, useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { commands } from '../ipc/commands';
import { isInviteLink } from '../ipc/inviteLink';
import type { Instance, LoaderKind, NewInstance, PackAddRequest, Settings } from '../ipc/schema';
import { nextPoll, pollServerStatus, strikesOf } from './serverPoll';
import { pushError } from './toasts';

export function createQueryClient() {
  return new QueryClient({
    defaultOptions: {
      queries: { retry: false, refetchOnWindowFocus: false, staleTime: 30_000 },
      mutations: { retry: false },
    },
  });
}

export const queryClient = createQueryClient();

export const qk = {
  appInfo: ['appInfo'] as const,
  settings: ['settings'] as const,
  instances: ['instances'] as const,
  accounts: ['accounts'] as const,
  mcVersions: ['catalog', 'minecraft'] as const,
  loaders: (kind: LoaderKind, mc: string) => ['catalog', 'loaders', kind, mc] as const,
  mods: (id: string) => ['mods', id] as const,
  packStatus: (id: string) => ['pack', 'status', id] as const,
  packContents: (id: string) => ['pack', 'contents', id] as const,
  packArt: (id: string) => ['pack', 'art', id] as const,
  manual: (id: string) => ['pack', 'manual', id] as const,
  joinPreview: (invite: string) => ['join', 'preview', invite] as const,
  serverStatus: (id: string) => ['server', 'status', id] as const,
};

const TEN_MINUTES = 10 * 60_000;

export const useAppInfo = () => useQuery({ queryKey: qk.appInfo, queryFn: commands.appInfo, staleTime: Infinity });
export const useSettings = () => useQuery({ queryKey: qk.settings, queryFn: commands.settingsGet });
export const useInstances = () => useQuery({ queryKey: qk.instances, queryFn: commands.instancesList });
export const useAccounts = () => useQuery({ queryKey: qk.accounts, queryFn: commands.accountsList });
export const useMcVersions = (enabled = true) =>
  useQuery({ queryKey: qk.mcVersions, queryFn: commands.catalogMinecraft, staleTime: TEN_MINUTES, enabled });
export const useLoaderVersions = (kind: LoaderKind | null, mc: string | null) =>
  useQuery({
    queryKey: qk.loaders(kind ?? 'fabric', mc ?? ''),
    queryFn: () => commands.catalogLoaders(kind as LoaderKind, mc as string),
    enabled: kind !== null && !!mc,
    staleTime: TEN_MINUTES,
  });
export const useMods = (id: string) => useQuery({ queryKey: qk.mods(id), queryFn: () => commands.modsList(id) });
export const usePackStatus = (id: string, enabled: boolean) =>
  useQuery({ queryKey: qk.packStatus(id), queryFn: () => commands.packStatus(id), enabled, staleTime: TEN_MINUTES });
export const usePackContents = (id: string, enabled: boolean) =>
  useQuery({ queryKey: qk.packContents(id), queryFn: () => commands.packContents(id), enabled });
export const usePackArt = (id: string, enabled: boolean) =>
  useQuery({ queryKey: qk.packArt(id), queryFn: () => commands.packArt(id), enabled, staleTime: Infinity });

export const useJoinPreview = (invite: string) =>
  useQuery({
    queryKey: qk.joinPreview(invite),
    queryFn: () => commands.joinPreview(invite),
    enabled: isInviteLink(invite),
    staleTime: 15_000,
  });

/** Live status, polled every `base` ms (backing off after rate limits) while the window is in front. */
export const useServerStatus = (id: string, enabled: boolean, base: number) =>
  useQuery({
    queryKey: qk.serverStatus(id),
    queryFn: () => pollServerStatus(id),
    enabled,
    staleTime: 10_000,
    refetchInterval: () => nextPoll(base, strikesOf(id)),
    refetchIntervalInBackground: false,
  });

interface Opts {
  /** The caller shows the error itself (forms); otherwise it becomes a toast. */
  inline?: boolean;
}
const onError = (opts: Opts) => (error: unknown) => {
  if (!opts.inline) pushError(error);
};

export function useSaveSettings(opts: Opts = {}) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (settings: Settings) => commands.settingsSet(settings),
    onSuccess: (saved) => qc.setQueryData(qk.settings, saved),
    onError: onError(opts),
  });
}

export function useCreateInstance(opts: Opts = {}) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (request: NewInstance) => commands.instanceCreate(request),
    onSuccess: () => qc.invalidateQueries({ queryKey: qk.instances }),
    onError: onError(opts),
  });
}

export function useAddPack(opts: Opts = {}) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (request: PackAddRequest) => commands.packAdd(request),
    onSuccess: () => qc.invalidateQueries({ queryKey: qk.instances }),
    onError: onError(opts),
  });
}

export function useJoinAdd(opts: Opts = {}) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (link: string) => commands.joinAdd(link),
    onSuccess: () => qc.invalidateQueries({ queryKey: qk.instances }),
    onError: onError(opts),
  });
}

export function useServerRelink(id: string, opts: Opts = {}) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (link: string) => commands.serverRelink(id, link),
    onSuccess: () => {
      for (const queryKey of [qk.instances, qk.serverStatus(id), qk.packStatus(id)]) void qc.invalidateQueries({ queryKey });
    },
    onError: onError(opts),
  });
}

export function usePackOptional(id: string, opts: Opts = {}) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ path, enabled }: { path: string; enabled: boolean }) => commands.packSetOptional(id, path, enabled),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: qk.packContents(id) });
      void qc.invalidateQueries({ queryKey: qk.instances });
    },
    onError: onError(opts),
  });
}

export function useUpdateInstance(opts: Opts = {}) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (instance: Instance) => commands.instanceUpdate(instance),
    onSuccess: () => qc.invalidateQueries({ queryKey: qk.instances }),
    onError: onError(opts),
  });
}

export function useRenameInstance(opts: Opts = {}) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ id, name }: { id: string; name: string }) => commands.instanceRename(id, name),
    onSuccess: () => qc.invalidateQueries({ queryKey: qk.instances }),
    onError: onError(opts),
  });
}

export function useDeleteInstance(opts: Opts = {}) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => commands.instanceDelete(id),
    onSuccess: () => qc.invalidateQueries({ queryKey: qk.instances }),
    onError: onError(opts),
  });
}

export function useAccountMutations(opts: Opts = {}) {
  const qc = useQueryClient();
  const onSuccess = (file: Awaited<ReturnType<typeof commands.accountsList>>) => qc.setQueryData(qk.accounts, file);
  // A failed change may have been made against a stale list, so the list is asked for again.
  const failed = (error: unknown) => {
    void qc.invalidateQueries({ queryKey: qk.accounts });
    onError(opts)(error);
  };
  return {
    addOffline: useMutation({ mutationFn: commands.accountAddOffline, onSuccess, onError: failed }),
    setActive: useMutation({ mutationFn: commands.accountSetActive, onSuccess, onError: failed }),
    remove: useMutation({ mutationFn: commands.accountRemove, onSuccess, onError: failed }),
  };
}

export function useModMutations(id: string, opts: Opts = {}) {
  const qc = useQueryClient();
  const onSuccess = (list: Awaited<ReturnType<typeof commands.modsList>>) => {
    qc.setQueryData(qk.mods(id), list);
    // A pack instance's additions list follows the mods folder.
    void qc.invalidateQueries({ queryKey: qk.packContents(id) });
  };
  // A failed change may still have touched the folder (a partial add), so the list is asked for again.
  const failed = (error: unknown) => {
    void qc.invalidateQueries({ queryKey: qk.mods(id) });
    void qc.invalidateQueries({ queryKey: qk.packContents(id) });
    onError(opts)(error);
  };
  return {
    toggle: useMutation({
      mutationFn: ({ file, enabled }: { file: string; enabled: boolean }) => commands.modSetEnabled(id, file, enabled),
      onSuccess,
      onError: failed,
    }),
    add: useMutation({ mutationFn: (paths: string[]) => commands.modsAdd(id, paths), onSuccess, onError: failed }),
    remove: useMutation({ mutationFn: (file: string) => commands.modRemove(id, file), onSuccess, onError: failed }),
  };
}
