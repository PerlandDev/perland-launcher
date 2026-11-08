// Perland's skin catalog for the Skins screen: the manifest and its status, and its PNGs loaded only when something
// asks for them and kept for the session (thumbnails are asked for by the grid as it draws, art and masks only when a
// look is shown on the figure or added).
import { useCallback, useState } from 'react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { commands } from '../../ipc/commands';
import type { CatalogManifest, CatalogStatus, SkinLook, SkinModel } from '../../ipc/schema';
import { decodeTexture } from './codec';
import type { ComposeLayer } from './compose';
import type { PixelBuffer } from './recolour';

const fileKey = (sha256: string) => ['skinCatalogFile', sha256] as const;

/** A catalog file as a data URL, fetched once per session (the files are named by their hash, so they never change). */
function fetchFile(qc: ReturnType<typeof useQueryClient>, sha256: string): Promise<string> {
  return qc.ensureQueryData({ queryKey: fileKey(sha256), queryFn: () => commands.skinCatalogFile(sha256), staleTime: Infinity, gcTime: Infinity });
}

/** Decoded texels by hash; shared by every mount, as the hashes are. */
const decoded = new Map<string, Promise<PixelBuffer>>();

export interface Catalog {
  manifest: CatalogManifest | null;
  status: CatalogStatus;
  /** True until the first answer. */
  loading: boolean;
  /** The file as a data URL once it's loaded; asking starts the load, and the component draws again when it lands. */
  image(sha256: string): string | undefined;
  /** The file's 64 × 64 texels. */
  pixels(sha256: string): Promise<PixelBuffer>;
  /** The look's worn pieces for `model`, ready for `compose`; pieces the manifest doesn't know are left out. */
  layers(look: SkinLook, model: SkinModel): Promise<ComposeLayer[]>;
}

export function useCatalog(): Catalog {
  const qc = useQueryClient();
  const answer = useQuery({ queryKey: ['skinCatalog'], queryFn: commands.skinCatalog, staleTime: 5 * 60_000, retry: false });
  const manifest = answer.data?.manifest ?? null;
  // The catalog command answers even with no network, so a failed call is the same as nothing to show.
  const status: CatalogStatus = answer.data?.status ?? 'unavailable';
  const [, landed] = useState(0);
  const asked = useState(() => new Set<string>())[0];

  const image = useCallback(
    (sha256: string) => {
      const url = qc.getQueryData<string>(fileKey(sha256));
      if (url !== undefined || asked.has(sha256)) return url;
      asked.add(sha256);
      // Started after this render, never during it.
      queueMicrotask(() =>
        void fetchFile(qc, sha256)
          .then(() => landed((n) => n + 1))
          .catch(() => asked.delete(sha256)),
      );
      return undefined;
    },
    [qc, asked],
  );

  const pixels = useCallback(
    (sha256: string) => {
      let p = decoded.get(sha256);
      if (!p) {
        p = fetchFile(qc, sha256).then(decodeTexture);
        p.catch(() => decoded.delete(sha256));
        decoded.set(sha256, p);
      }
      return p;
    },
    [qc],
  );

  const layers = useCallback(
    async (look: SkinLook, model: SkinModel) => {
      if (!manifest) return [];
      const byId = new Map(manifest.pieces.map((p) => [p.id, p]));
      const worn = look.pieces.flatMap((w) => {
        const piece = byId.get(w.id);
        const files = piece?.files[model];
        return piece && files ? [{ piece, files, worn: w }] : [];
      });
      return Promise.all(
        worn.map(async ({ piece, files, worn: w }): Promise<ComposeLayer> => {
          const [art, mask] = await Promise.all([pixels(files.art), pixels(files.mask)]);
          return { slot: piece.slot, art, mask, colours: piece.areas.map((a) => w.colours[a.id] ?? a.default) };
        }),
      );
    },
    [manifest, pixels],
  );

  return { manifest, status, loading: answer.isPending, image, pixels, layers };
}
