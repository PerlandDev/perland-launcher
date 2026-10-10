import { useState } from 'react';
import type { GameSpec, LoaderSpec } from '../../ipc/schema';
import { useLoaderVersions, useMcVersions } from '../../state/queries';

export type LoaderChoice = 'none' | 'fabric' | 'quilt';

/** Version and loader state for a form. Unset choices resolve to the newest release and the
 *  newest stable loader, derived on render (no effects), so defaults follow the lists as they load. */
export function useVersionChoice(initial?: GameSpec) {
  const versions = useMcVersions();
  const [snapshots, setSnapshots] = useState(false);
  const [minecraft, setMinecraft] = useState<string | null>(initial?.minecraft ?? null);
  // Forge and NeoForge are not offered yet: an instance that has one starts as "None" with no stale version.
  const initialKind = initial?.loader?.kind === 'fabric' || initial?.loader?.kind === 'quilt' ? initial.loader.kind : null;
  const [loader, setLoader] = useState<LoaderChoice>(initialKind ?? 'none');
  const [loaderVersion, setLoaderVersion] = useState<string | null>(initialKind ? (initial?.loader?.version ?? null) : null);

  const all = versions.data ?? [];
  const list = all.filter((v) => v.kind === 'release' || (snapshots && v.kind === 'snapshot') || v.id === minecraft);
  const newestRelease = list.find((v) => v.kind === 'release')?.id ?? list[0]?.id ?? null;
  // A version the instance already has is kept even if the catalog no longer lists it (the form
  // adds it as an extra option); only an unset choice falls back to the newest release.
  const mc = minecraft ?? newestRelease;

  const loaders = useLoaderVersions(loader === 'none' ? null : loader, mc);
  const loaderList = loaders.data ?? [];
  // A version the player (or the instance) already has is kept even if the list no longer offers
  // it; only an unset choice falls back to the newest stable loader.
  const chosenLoader = loaderVersion ?? ((loaderList.find((v) => v.stable) ?? loaderList[0])?.version ?? null);

  const resolvedLoader: LoaderSpec | null = loader === 'none' || !chosenLoader ? null : { kind: loader, version: chosenLoader };
  return {
    versions,
    loaders,
    snapshots,
    setSnapshots,
    list,
    mc,
    setMinecraft: (next: string) => {
      setMinecraft(next);
      setLoaderVersion(null);
    },
    loader,
    setLoader: (next: LoaderChoice) => {
      setLoader(next);
      setLoaderVersion(null);
    },
    loaderList,
    chosenLoader,
    setLoaderVersion,
    resolved: {
      minecraft: mc,
      loader: resolvedLoader,
      ready: Boolean(mc) && (loader === 'none' || resolvedLoader !== null),
    },
  };
}

export type VersionChoice = ReturnType<typeof useVersionChoice>;
