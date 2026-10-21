import { QueryClient, useInfiniteQuery, useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import i18n from '../i18n';
import { commands } from '../ipc/commands';
import { isLauncherError } from '../ipc/errors';
import { isInviteLink } from '../ipc/inviteLink';
import type { ArtChoice, ManageListChange, ManageSettingsSave, ManageState, ContentKind, Focus, FreeServerKind, FreeServerPage, IconChoice, ImageKind, ImportRequest, Instance, InstanceListing, ListQuery, LoaderKind, NewInstance, PackAddRequest, SearchRequest, ServerStats, Settings } from '../ipc/schema';
import { nextPoll, pollServerStatus, strikesOf } from './serverPoll';
import { lastInstance, navigate, rememberInstance, routeSnapshot, type Route } from './route';
import { endImport, importWatched } from './importProgress';
import { pushError, pushNotice } from './toasts';

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
  update: ['update'] as const,
  instances: ['instances'] as const,
  accounts: ['accounts'] as const,
  playtime: ['playtime'] as const,
  mcVersions: ['catalog', 'minecraft'] as const,
  loaders: (kind: LoaderKind, mc: string) => ['catalog', 'loaders', kind, mc] as const,
  mods: (id: string) => ['mods', id] as const,
  contentFiles: (id: string, kind: ContentKind) => ['content', id, kind] as const,
  packStatus: (id: string) => ['pack', 'status', id] as const,
  packContents: (id: string) => ['pack', 'contents', id] as const,
  instanceArt: (id: string) => ['instance', 'art', id] as const,
  instanceScreenshots: (id: string) => ['instance', 'screenshots', id] as const,
  instanceScreenshot: (id: string, name: string) => ['instance', 'screenshots', id, name] as const,
  manual: (id: string) => ['pack', 'manual', id] as const,
  browseSearch: (request: Omit<SearchRequest, 'offset'>) => ['browse', 'search', request] as const,
  browseCategories: (kind: ContentKind) => ['browse', 'categories', kind] as const,
  browseProject: (id: string) => ['browse', 'project', id] as const,
  browseVersions: (id: string) => ['browse', 'versions', id] as const,
  browseInstalled: (id: string) => ['browse', 'installed', id] as const,
  browseImage: (url: string, kind: ImageKind) => ['browse', 'image', kind, url] as const,
  // Apart from qk.mods, so a change to the list doesn't ask every row's icon again.
  modIcon: (id: string, file: string, version: number) => ['modIcon', id, file, version] as const,
  joinPreview: (invite: string) => ['join', 'preview', invite] as const,
  serverStatus: (id: string) => ['server', 'status', id] as const,
  servers: (query: ListQuery) => ['servers', query] as const,
  perland: ['perland'] as const,
  perlandStats: ['perlandStats'] as const,
  yourServers: ['yourServers'] as const,
  freeServer: ['freeServer'] as const,
  serverIcon: (token: string, iconVersion: string) => ['serverIcon', token, iconVersion] as const,
  news: (invite: string) => ['news', invite] as const,
  newsAll: (invite: string) => ['newsAll', invite] as const,
  newsImage: (invite: string, postId: string, imageVersion: string) => ['newsImage', invite, postId, imageVersion] as const,
  manageDetail: (tenantId: string, stack: string) => ['manage', tenantId, stack, 'detail'] as const,
  manageDiagnosis: (tenantId: string, stack: string) => ['manage', tenantId, stack, 'diagnosis'] as const,
  managePlayers: (tenantId: string, stack: string) => ['manage', tenantId, stack, 'players'] as const,
  manageSettings: (tenantId: string, stack: string) => ['manage', tenantId, stack, 'settings'] as const,
};

const TEN_MINUTES = 10 * 60_000;

export const useAppInfo = () => useQuery({ queryKey: qk.appInfo, queryFn: commands.appInfo, staleTime: Infinity });
export const useSettings = () => useQuery({ queryKey: qk.settings, queryFn: commands.settingsGet });
export const useUpdate = () => useQuery({ queryKey: qk.update, queryFn: commands.updateStatus });
export const useInstances = () => useQuery({ queryKey: qk.instances, queryFn: commands.instancesList });
export const useAccounts = () => useQuery({ queryKey: qk.accounts, queryFn: commands.accountsList });
/** Time played per instance on this computer. Refreshed when a run ends and when an instance moves or goes. */
export const usePlaytime = () => useQuery({ queryKey: qk.playtime, queryFn: commands.playtimeList });
/** The active account's name, or null while loading or when there is none. */
export function useActiveAccountName(): string | null {
  const { data } = useAccounts();
  return data?.accounts.find((a) => a.uuid === data.active)?.username ?? null;
}
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
/** A mod's own icon (`null`: it has none). `version` (the jar's size) asks again when a jar is replaced. */
export const useModIcon = (id: string, file: string | null, version: number | undefined, enabled: boolean) =>
  useQuery({
    queryKey: qk.modIcon(id, file ?? '', version ?? 0),
    queryFn: () => commands.modIcon(id, file as string),
    enabled: enabled && file !== null,
    staleTime: Infinity,
  });
export const useContentFiles = (id: string, kind: ContentKind) =>
  useQuery({ queryKey: qk.contentFiles(id, kind), queryFn: () => commands.contentList(id, kind) });
export const usePackStatus = (id: string, enabled: boolean) =>
  useQuery({ queryKey: qk.packStatus(id), queryFn: () => commands.packStatus(id), enabled, staleTime: TEN_MINUTES });
export const usePackContents = (id: string, enabled: boolean) =>
  useQuery({ queryKey: qk.packContents(id), queryFn: () => commands.packContents(id), enabled });
export const useInstanceArt = (id: string) =>
  useQuery({ queryKey: qk.instanceArt(id), queryFn: () => commands.instanceArt(id), staleTime: Infinity });
/** The instance's newest screenshots by name, read each time "Change image…" opens (the game adds more while it runs). */
export const useInstanceScreenshots = (id: string) =>
  useQuery({ queryKey: qk.instanceScreenshots(id), queryFn: () => commands.instanceScreenshots(id), staleTime: 0 });
/** One screenshot as a data: URL, read only when its tile shows. A file never changes under its name. */
export const useInstanceScreenshot = (id: string, name: string) =>
  useQuery({ queryKey: qk.instanceScreenshot(id, name), queryFn: () => commands.instanceScreenshot(id, name), staleTime: Infinity });

/** The public list's page size, fixed by the Engine. */
export const SERVERS_PAGE = 50;
/** The public server list for `query` (its `offset` is the page param). It refetches every 30 s. */
export const useServersList = (query: ListQuery) =>
  useInfiniteQuery({
    queryKey: qk.servers(query),
    queryFn: ({ pageParam }) => commands.serversList({ ...query, offset: pageParam }),
    initialPageParam: 0,
    getNextPageParam: (_last, pages) => {
      const next = pages.length * SERVERS_PAGE;
      return next < (pages.at(-1)?.total ?? 0) ? next : undefined;
    },
    refetchInterval: 30_000,
  });
/** Whether player accounts are on, the ways to sign in, and who is signed in. Changed by the sign-in commands, which set it. */
export const usePerlandStatus = () => useQuery({ queryKey: qk.perland, queryFn: commands.perlandStatus });
/** Stands in for the linked name in the stats' key while signed out. A Minecraft name has no hyphen, so it never clashes. */
const SIGNED_OUT = 'signed-out';
/** The linked player's numbers per server; null while not linked or with the Engine's add-on off, and no data at all (undefined)
 *  until the status says who is signed in. The linked name is in the key, so a link made or removed elsewhere (seen through
 *  `perland_status`) reads again; while signed out the key is one that is never read, so a sign-out the status sees first leaves no
 *  numbers behind. `qk.perlandStats` still matches every key as a prefix. Its error is never toasted: the numbers are a bonus. */
export function usePerlandStats() {
  const account = usePerlandStatus().data?.account;
  return useQuery({
    queryKey: [...qk.perlandStats, account ? account.minecraft : SIGNED_OUT],
    queryFn: commands.perlandStats,
    enabled: Boolean(account),
    staleTime: Infinity,
  });
}
/** The stats of the server with `token`, or undefined while there are none. */
export const useServerStats = (token: string): ServerStats | undefined =>
  usePerlandStats().data?.servers.find((s) => s.token === token);
/** The stats of the server that the instance `id` joins, or undefined. */
export const useInstanceStats = (id: string): ServerStats | undefined => usePerlandStats().data?.servers.find((s) => s.instance === id);
/** The player's servers (null while signed out). Reads only when `enabled` (signed in) and refetches every minute while mounted;
 *  invalidate `qk.yourServers` after a follow, an unfollow or a join. */
export const useYourServers = (enabled: boolean) =>
  useQuery({ queryKey: qk.yourServers, queryFn: commands.yourServers, enabled, refetchInterval: 60_000 });
/** How often the free-server page is read: every 30 s, every 15 s while the server is being created, started or deleted. */
export const freeServerInterval = (page: FreeServerPage | null | undefined): number => {
  const status = page?.server?.freeStatus;
  return status === 'creating' || status === 'starting' || status === 'deleting' ? 15_000 : 30_000;
};
/** The free-server page (null while signed out or when the feature is off). Reads only when `enabled` (signed in, Servers open). */
export const useFreeServer = (enabled: boolean) =>
  useQuery({
    queryKey: qk.freeServer,
    queryFn: commands.freeServer,
    enabled,
    refetchInterval: (query) => freeServerInterval(query.state.data),
  });
/** A card's icon (a data URL), or null. A new `iconVersion` is a new key; it is only asked for when there is one. */
export const useServerIcon = (token: string, iconVersion: string | null) =>
  useQuery({
    queryKey: qk.serverIcon(token, iconVersion ?? ''),
    queryFn: () => commands.serversIcon(token, iconVersion as string),
    staleTime: Infinity,
    enabled: !!iconVersion,
  });

export const BROWSE_PAGE = 20;
const FIVE_MINUTES = 5 * 60_000;

/** A page of server news, for the line on a card or on Home and for the dialog. */
export const NEWS_PAGE = 20;
/** The newest page of a server's news, or null when it has none. Errors render nothing: news is a bonus, never a blocker.
 *  `enabled` is true while the card is on screen or Home is open. Read for 5 minutes. */
export const useServerNews = (invite: string | null, enabled: boolean) =>
  useQuery({
    queryKey: qk.news(invite ?? ''),
    queryFn: () => commands.serverNews(invite as string, null, NEWS_PAGE),
    enabled: invite !== null && enabled,
    staleTime: FIVE_MINUTES,
    refetchInterval: FIVE_MINUTES,
    retry: false,
  });
/** The news dialog's pages, 20 a page, each continuing from the last page's `nextBefore`. Reads only while `open`. */
export const useServerNewsPages = (invite: string | null, open: boolean) =>
  useInfiniteQuery({
    queryKey: qk.newsAll(invite ?? ''),
    queryFn: ({ pageParam }) => commands.serverNews(invite as string, pageParam, NEWS_PAGE),
    initialPageParam: null as string | null,
    getNextPageParam: (last) => last?.nextBefore ?? undefined,
    enabled: invite !== null && open,
    staleTime: FIVE_MINUTES,
    retry: false,
  });
/** A post's picture (a data URL), or null. A new `imageVersion` is a new key; it is only asked for when there is one. */
export const useNewsImage = (invite: string, postId: string, imageVersion: string | null) =>
  useQuery({
    queryKey: qk.newsImage(invite, postId, imageVersion ?? ''),
    queryFn: () => commands.newsImage(invite, postId, imageVersion as string),
    staleTime: Infinity,
    enabled: !!imageVersion,
  });

export const useBrowseSearch = (request: Omit<SearchRequest, 'offset'>) =>
  useInfiniteQuery({
    queryKey: qk.browseSearch(request),
    queryFn: ({ pageParam }) => commands.browseSearch({ ...request, offset: pageParam }),
    initialPageParam: 0,
    // The core drops server-only hits, so a page can be short; step by the page size.
    getNextPageParam: (last) => (last.offset + BROWSE_PAGE < last.total ? last.offset + BROWSE_PAGE : undefined),
    staleTime: FIVE_MINUTES,
  });
export const useBrowseCategories = (kind: ContentKind) =>
  useQuery({ queryKey: qk.browseCategories(kind), queryFn: () => commands.browseCategories(kind), staleTime: Infinity });
export const useBrowseProject = (id: string) =>
  useQuery({ queryKey: qk.browseProject(id), queryFn: () => commands.browseProject(id), staleTime: FIVE_MINUTES });
export const useBrowseVersions = (id: string) =>
  useQuery({ queryKey: qk.browseVersions(id), queryFn: () => commands.browseVersions(id, null), staleTime: FIVE_MINUTES });
export const useBrowseInstalled = (id: string | null) =>
  useQuery({ queryKey: qk.browseInstalled(id ?? ''), queryFn: () => commands.browseInstalled(id as string), enabled: id !== null });
export const useBrowseImage = (url: string | null, kind: ImageKind) =>
  useQuery({
    queryKey: qk.browseImage(url ?? '', kind),
    queryFn: () => commands.browseImage(url as string, kind),
    enabled: url !== null,
    staleTime: Infinity,
  });

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

/** Follow or joined: fire and forget. It never reports or waits (signed out it does nothing), and Your servers reads again after it. */
export function recordYourServer(qc: QueryClient, call: () => Promise<unknown>) {
  void call()
    .catch(() => {})
    .finally(() => void qc.invalidateQueries({ queryKey: qk.yourServers }));
}

/** Unfollow, the one Your servers call that reports: its error is a toast. Your servers reads again either way. */
export function useUnfollow() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (invite: string) => commands.yourServersUnfollow(invite),
    onError: pushError,
    onSettled: () => qc.invalidateQueries({ queryKey: qk.yourServers }),
  });
}

/** What a free-server change touches: its own page, and Your servers (its card and the owner entry). */
const invalidateFree = (qc: QueryClient) =>
  Promise.all([qc.invalidateQueries({ queryKey: qk.freeServer }), qc.invalidateQueries({ queryKey: qk.yourServers })]);

export interface FreeServerRequest {
  name: string;
  kind: FreeServerKind;
  modrinthVersionId: string | null;
  language: string;
}
/** Creates the free server (the call can take a couple of minutes). The dialog shows the error itself, so pass `inline`. */
export function useFreeServerCreate(opts: Opts = {}) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (r: FreeServerRequest) => commands.freeServerCreate(r.name, r.kind, r.modrinthVersionId, r.language),
    onError: onError(opts),
    onSettled: () => invalidateFree(qc),
  });
}
export function useFreeServerStart(opts: Opts = {}) {
  const qc = useQueryClient();
  return useMutation({ mutationFn: commands.freeServerStart, onError: onError(opts), onSettled: () => invalidateFree(qc) });
}
export function useFreeServerDelete(opts: Opts = {}) {
  const qc = useQueryClient();
  return useMutation({ mutationFn: commands.freeServerDelete, onError: onError(opts), onSettled: () => invalidateFree(qc) });
}
/** Starts and joins the free server and answers the instance id. The caller starts the run with `commands.play(id, true)` right after. */
export function useFreeServerPlay(opts: Opts = {}) {
  const qc = useQueryClient();
  return useMutation({ mutationFn: commands.freeServerPlay, onError: onError(opts), onSettled: () => invalidateFree(qc) });
}

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

/** What a folder from another launcher holds. The form shows the error itself. */
export function useImportInspect() {
  return useMutation({ mutationFn: (path: string) => commands.importInspect(path) });
}

/** The import's whole ending lives here, not in the form: the copy goes on when the player leaves
 *  it. A form on screen opens the new instance or shows the failure; otherwise a toast tells. */
export function useImportInstance() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (request: ImportRequest) => commands.importInstance(request),
    onSuccess: (instance) => {
      const watched = importWatched();
      endImport();
      if (watched) navigate({ name: 'home', id: instance.id });
      else pushNotice({ title: i18n.t('import.done', { name: instance.name }) });
      return qc.invalidateQueries({ queryKey: qk.instances });
    },
    onError: (error) => {
      // The player's own cancel isn't a failure: nothing to tell.
      const cancelled = isLauncherError(error) && error.code === 'import_cancelled';
      const watched = importWatched();
      endImport(cancelled ? null : error);
      if (!watched && !cancelled) pushError(error);
    },
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

/** Unlink: the instance keeps its files and stops following its server. */
export function useServerUnlink(id: string, opts: Opts = {}) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: () => commands.serverUnlink(id),
    onSuccess: (instance) => {
      qc.setQueryData<InstanceListing>(qk.instances, (old) =>
        old ? { ...old, instances: old.instances.map((i) => (i.id === instance.id ? instance : i)) } : old,
      );
      for (const queryKey of [qk.instances, qk.packStatus(id)]) void qc.invalidateQueries({ queryKey });
      qc.removeQueries({ queryKey: qk.serverStatus(id) });
    },
    onError: onError(opts),
  });
}

export function useServerLink(id: string, opts: Opts = {}) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (link: string) => commands.serverLink(id, link),
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

/** The same route, pointed at `to` wherever it named the instance `from`; null when it never did. */
function routeFollowing(route: Route, from: string, to: string): Route | null {
  switch (route.name) {
    case 'home':
    case 'mods':
    case 'instanceSettings':
      return route.id === from ? { ...route, id: to } : null;
    case 'browse':
      return route.instance === from ? { ...route, instance: to } : null;
    case 'joinServer':
      if (route.relinkFor === from) return { ...route, relinkFor: to };
      return route.linkFor === from ? { ...route, linkFor: to } : null;
    default:
      return null;
  }
}

export function useRenameInstance(opts: Opts = {}) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ id, name }: { id: string; name: string }) => commands.instanceRename(id, name),
    onSuccess: (renamed, { id }) => {
      if (renamed.id !== id) {
        // The folder (and so the id) moved with the name. Put the new id in the cached listing before the route follows, so the screen never falls back to another instance.
        qc.setQueryData<InstanceListing>(qk.instances, (listing) =>
          listing && { ...listing, instances: listing.instances.map((i) => (i.id === id ? renamed : i)) },
        );
        const next = routeFollowing(routeSnapshot(), id, renamed.id);
        if (next) navigate(next);
        else if (lastInstance() === id) rememberInstance(renamed.id);
        void qc.invalidateQueries({ queryKey: qk.instanceArt(id) });
        void qc.invalidateQueries({ queryKey: qk.serverStatus(id) });
        void qc.invalidateQueries({ queryKey: qk.playtime });
      }
      return qc.invalidateQueries({ queryKey: qk.instances });
    },
    onError: onError(opts),
  });
}

/** The background, the icon and the background's position are saved one after another: each rewrites the
 *  instance's file, so two at once could undo each other. */
const PICTURE_SAVES = { id: 'instance-pictures' };

/** Change an instance's picture. The cached listing takes the saved instance at once, so the
 *  picker's lamp moves without waiting; the art and the listing are read again so the rail icon
 *  and the marquee follow. */
export function useSetInstanceArt(opts: Opts = {}) {
  const qc = useQueryClient();
  return useMutation({
    scope: PICTURE_SAVES,
    mutationFn: ({ id, choice }: { id: string; choice: ArtChoice }) => commands.instanceArtSet(id, choice),
    onSuccess: (saved, { id }) => {
      qc.setQueryData<InstanceListing>(qk.instances, (listing) =>
        listing && { ...listing, instances: listing.instances.map((i) => (i.id === id ? saved : i)) },
      );
      void qc.invalidateQueries({ queryKey: qk.instanceArt(id) });
      return qc.invalidateQueries({ queryKey: qk.instances });
    },
    onError: onError(opts),
  });
}

/** Change an instance's icon, apart from its background. Like `useSetInstanceArt`: the cached
 *  listing takes the saved instance at once, then the art and the listing are read again. */
export function useSetInstanceIcon(opts: Opts = {}) {
  const qc = useQueryClient();
  return useMutation({
    scope: PICTURE_SAVES,
    mutationFn: ({ id, choice }: { id: string; choice: IconChoice }) => commands.instanceIconSet(id, choice),
    onSuccess: (saved, { id }) => {
      qc.setQueryData<InstanceListing>(qk.instances, (listing) =>
        listing && { ...listing, instances: listing.instances.map((i) => (i.id === id ? saved : i)) },
      );
      void qc.invalidateQueries({ queryKey: qk.instanceArt(id) });
      return qc.invalidateQueries({ queryKey: qk.instances });
    },
    onError: onError(opts),
  });
}

/** Show a background position on the marquee before it is saved: the cached listing takes it at once,
 *  so the marquee behind "Change image…" follows a drag. `useSetBannerFocus` saves it. */
export function usePreviewBannerFocus() {
  const qc = useQueryClient();
  return (id: string, focus: Focus | null) =>
    qc.setQueryData<InstanceListing>(qk.instances, (listing) =>
      listing && { ...listing, instances: listing.instances.map((i) => (i.id === id ? { ...i, bannerFocus: focus } : i)) },
    );
}

/** Save which part of a picture background stays in view (null is the default). The marquee already shows
 *  it (`usePreviewBannerFocus`), so success leaves the cache alone: a newer position may be on screen by
 *  then. A refused save reads the listing again, so the marquee goes back to what is on disk. */
export function useSetBannerFocus(opts: Opts = {}) {
  const qc = useQueryClient();
  return useMutation({
    scope: PICTURE_SAVES,
    mutationFn: ({ id, focus }: { id: string; focus: Focus | null }) => commands.instanceBannerFocusSet(id, focus),
    onError: (error) => {
      onError(opts)(error);
      return qc.invalidateQueries({ queryKey: qk.instances });
    },
  });
}

/** Save the player's rail order. The cached listing takes the new order at once and goes back if
 *  the core refuses it; the core's own listing replaces it on success. Moves run one at a time, so
 *  quick Alt+arrow presses reach the core in the order they were made. */
export function useReorderInstances(opts: Opts = {}) {
  const qc = useQueryClient();
  return useMutation({
    scope: { id: 'instance-order' },
    mutationFn: (ids: string[]) => commands.instanceReorder(ids),
    onMutate: async (ids) => {
      await qc.cancelQueries({ queryKey: qk.instances });
      const previous = qc.getQueryData<InstanceListing>(qk.instances);
      if (previous) {
        const byId = new Map(previous.instances.map((i) => [i.id, i]));
        const ordered = ids.flatMap((id) => byId.get(id) ?? []);
        // Anything the ids missed (an instance added meanwhile) stays on top, like the core's rule.
        const rest = previous.instances.filter((i) => !ids.includes(i.id));
        qc.setQueryData<InstanceListing>(qk.instances, { ...previous, instances: [...rest, ...ordered] });
      }
      return { previous };
    },
    onSuccess: (listing) => qc.setQueryData(qk.instances, listing),
    onError: (error, _ids, context) => {
      if (context?.previous) qc.setQueryData(qk.instances, context.previous);
      onError(opts)(error);
    },
  });
}

export function useDeleteInstance(opts: Opts = {}) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => commands.instanceDelete(id),
    onSuccess: () => {
      // The instance's time goes with it, so the Accounts total drops too.
      void qc.invalidateQueries({ queryKey: qk.playtime });
      return qc.invalidateQueries({ queryKey: qk.instances });
    },
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

/** Remove a resource pack or shader. The list the core hands back replaces the cached one. */
export function useContentRemove(id: string, kind: ContentKind, opts: Opts = {}) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (file: string) => commands.contentRemove(id, kind, file),
    onSuccess: (list) => qc.setQueryData(qk.contentFiles(id, kind), list),
    // A refused removal may still have changed the folder, so the list is asked for again.
    onError: (error) => {
      void qc.invalidateQueries({ queryKey: qk.contentFiles(id, kind) });
      onError(opts)(error);
    },
  });
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

/** States that change on their own soon: the detail is asked every 5 s while one lasts, else every 30 s. */
const MOVING: readonly ManageState[] = ['starting', 'stopping'];

/** A managed server's state and who is on. Mounted only by the Manage screen, so it polls only while that is open. */
export const useManageDetail = (tenantId: string, stack: string) =>
  useQuery({
    queryKey: qk.manageDetail(tenantId, stack),
    queryFn: () => commands.manageDetail(tenantId, stack),
    // The overview and the tabs all read it: a tab opening doesn't ask again inside the fast poll's 5 s.
    staleTime: 4000,
    refetchInterval: (q) => (q.state.data && MOVING.includes(q.state.data.state) ? 5000 : 30000),
  });
/** Why the last run failed: asked once while the server is stopped; an action asks again. */
export const useManageDiagnosis = (tenantId: string, stack: string, enabled: boolean) =>
  useQuery({
    queryKey: qk.manageDiagnosis(tenantId, stack),
    queryFn: () => commands.manageDiagnosis(tenantId, stack),
    staleTime: Infinity,
    enabled,
  });
export const useManagePlayers = (tenantId: string, stack: string) =>
  useQuery({ queryKey: qk.managePlayers(tenantId, stack), queryFn: () => commands.managePlayers(tenantId, stack), staleTime: 0 });
export const useManageSettings = (tenantId: string, stack: string) =>
  useQuery({ queryKey: qk.manageSettings(tenantId, stack), queryFn: () => commands.manageSettings(tenantId, stack), staleTime: 0 });

/** Everything an action can change: the state, the players and why it failed. */
function invalidateManaged(qc: QueryClient, tenantId: string, stack: string) {
  return Promise.all([
    qc.invalidateQueries({ queryKey: qk.manageDetail(tenantId, stack) }),
    qc.invalidateQueries({ queryKey: qk.managePlayers(tenantId, stack) }),
    qc.invalidateQueries({ queryKey: qk.manageDiagnosis(tenantId, stack) }),
    qc.invalidateQueries({ queryKey: qk.yourServers }),
  ]);
}

/** Start, stop or restart. The screen shows a refusal itself, so pass `inline`. */
export function useManageAction(tenantId: string, stack: string, opts: Opts = {}) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (action: 'start' | 'stop' | 'restart') => commands.manageAction(tenantId, stack, action),
    onError: onError(opts),
    onSettled: () => invalidateManaged(qc, tenantId, stack),
  });
}
export function useManagePlayersChange(tenantId: string, stack: string, opts: Opts = {}) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (change: ManageListChange) => commands.managePlayersChange(tenantId, stack, change),
    onError: onError(opts),
    onSettled: () => qc.invalidateQueries({ queryKey: qk.managePlayers(tenantId, stack) }),
  });
}
export function useManageSettingsSave(tenantId: string, stack: string, opts: Opts = {}) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (save: ManageSettingsSave) => commands.manageSettingsSave(tenantId, stack, save),
    onError: onError(opts),
    onSuccess: (saved) => qc.setQueryData(qk.manageSettings(tenantId, stack), saved.settings),
    onSettled: () => qc.invalidateQueries({ queryKey: qk.manageDetail(tenantId, stack) }),
  });
}
/** One console command; `confirm` answers the Engine's question about a command that stops the server. */
export function useManageConsole(tenantId: string, stack: string, opts: Opts = {}) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ command, confirm }: { command: string; confirm: boolean }) => commands.manageConsole(tenantId, stack, command, confirm),
    onError: onError(opts),
    onSettled: () => qc.invalidateQueries({ queryKey: qk.manageDetail(tenantId, stack) }),
  });
}
/** Opens the Perland panel, already signed in, at the server's page (or the plans page with no server). */
export function openManagePanel(tenantId: string, stack: string | null) {
  return commands.manageOpenPanel(tenantId, stack).catch(pushError);
}
