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
  const [loader, setLoader] = useState<LoaderChoice>(
    initial?.loader?.kind === 'fabric' || initial?.loader?.kind === 'quilt' ? initial.loader.kind : 'none',
  );
  const [loaderVersion, setLoaderVersion] = useState<string | null>(initial?.loader?.version ?? null);

  const all = versions.data ?? [];
  const list = all.filter((v) => v.kind === 'release' || (snapshots && v.kind === 'snapshot') || v.id === minecraft);
  const mc = minecraft && (list.some((v) => v.id === minecraft) || !versions.data) ? minecraft : (list[0]?.id ?? minecraft);

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
