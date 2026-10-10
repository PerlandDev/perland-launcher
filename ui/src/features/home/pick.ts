import type { DamagedInstance, Instance, InstanceListing } from '../../ipc/schema';
import { lastInstance } from '../../state/route';

export type Picked =
  | { kind: 'instance'; instance: Instance }
  | { kind: 'damaged'; damaged: DamagedInstance }
  | { kind: 'empty' };

/** The instance a screen is about: the requested one, else the last one opened, else the first. */
export function pickId(listing: InstanceListing | undefined, requested: string | null): string | null {
  if (!listing) return null;
  const ids = [...listing.instances.map((i) => i.id), ...listing.damaged.map((d) => d.id)];
  if (requested && ids.includes(requested)) return requested;
  const last = lastInstance();
  if (last && ids.includes(last)) return last;
  return ids[0] ?? null;
}

export function pick(listing: InstanceListing | undefined, requested: string | null): Picked {
  const id = pickId(listing, requested);
  const instance = listing?.instances.find((i) => i.id === id);
  if (instance) return { kind: 'instance', instance };
  const damaged = listing?.damaged.find((d) => d.id === id);
  if (damaged) return { kind: 'damaged', damaged };
  return { kind: 'empty' };
}
