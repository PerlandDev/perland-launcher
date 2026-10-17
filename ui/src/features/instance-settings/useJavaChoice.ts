import { useQuery } from '@tanstack/react-query';
import { useState } from 'react';
import { commands } from '../../ipc/commands';
import type { Instance, JavaFound } from '../../ipc/schema';

/**
 * The instance's Java: Automatic (Mojang's runtime) or one the player picks from the Javas found on
 * this computer, or browses to. A Java that doesn't match what `minecraft` asks for is saved only
 * after the player says "use it anyway"; the one already stored counts as said.
 */
export function useJavaChoice(instance: Instance, minecraft: string, touched: () => void) {
  const [mode, setMode] = useState<'auto' | 'custom'>(instance.java.path ? 'custom' : 'auto');
  const [path, setPath] = useState<string | null>(instance.java.path);
  const stored = instance.java.path ? `${instance.java.path}|${instance.game.minecraft}` : null;
  const [acknowledged, setAcknowledged] = useState<string | null>(stored);
  const [pickError, setPickError] = useState<unknown>(null);

  const overview = useQuery({
    queryKey: ['javaOverview', minecraft],
    queryFn: () => commands.javaOverview(minecraft),
    staleTime: Infinity,
  });
  const listed = path ? overview.data?.installs.find((j) => j.path === path) : undefined;
  // A Java the list doesn't hold (browsed to, or stored and since moved) is checked on its own.
  const probe = useQuery({
    queryKey: ['javaTest', path, minecraft],
    queryFn: () => commands.javaTest(path!, minecraft),
    enabled: mode === 'custom' && path !== null && overview.isSuccess && !listed,
    retry: false,
    staleTime: Infinity,
  });
  const chosen: JavaFound | undefined = listed ?? probe.data;
  const key = path ? `${path}|${minecraft}` : null;
  const mismatch = mode === 'custom' && chosen?.fit != null && chosen.fit !== 'matches';
  const ready = mode === 'auto' || (chosen !== undefined && (!mismatch || acknowledged === key));

  return {
    mode,
    path,
    overview,
    probe: listed ? null : probe,
    chosen,
    mismatch,
    pickError,
    minecraft,
    acknowledged: acknowledged !== null && acknowledged === key,
    ready,
    /** What `instance.java.path` saves as. */
    value: mode === 'custom' ? path : null,
    setMode: (next: 'auto' | 'custom') => {
      touched();
      setMode(next);
    },
    choose: (next: string) => {
      touched();
      setPickError(null);
      setPath(next);
    },
    acknowledge: (yes: boolean) => {
      touched();
      setAcknowledged(yes ? key : null);
    },
    failPick: setPickError,
  };
}

export type JavaChoice = ReturnType<typeof useJavaChoice>;
