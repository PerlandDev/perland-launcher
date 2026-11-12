import { useRef, type KeyboardEvent, type ReactNode } from 'react';
import { useTranslation } from 'react-i18next';
import { navigate, useRoute, type Route } from '../../state/route';
import { Package, Server, Shirt } from '../../ui/Icon';
import { sessionKind } from '../browse/session';

interface Place {
  id: string;
  label: string;
  icon: ReactNode;
  go: () => void;
}

/** Which place a route belongs to, so its tab stays marked on the screens it leads to. An instance's own screens (its
 *  Home, Mods & packs, settings, Browse opened from it) belong to no place: the rail marks the instance. */
function placeOf(route: Route): string | null {
  switch (route.name) {
    case 'skins':
      return 'skins';
    case 'servers':
      return 'servers';
    case 'browse':
      return route.instance ? null : 'browse';
    default:
      return null;
  }
}

/** The launcher's places by name, as tabs in the title bar: Skins, Servers and Mods & more (Browse: the box is the
 *  launcher's sign for mods, as on Home's Mods & packs key). The player's own page, the Profile, opens from the account at
 *  the bar's other end. They sit here, not in the rail, so the rail stays the instances' alone. Flat title-bar text, never lit; the open one is
 *  pressed into its slot with the lime lamp, like a chosen segment. One Tab stop, walked with the arrow keys. */
export function Places() {
  const { t } = useTranslation();
  const route = useRoute();
  const buttons = useRef<(HTMLButtonElement | null)[]>([]);
  const open = placeOf(route);
  const places: Place[] = [
    { id: 'skins', label: t('titlebar.skins'), icon: <Shirt aria-hidden />, go: () => navigate({ name: 'skins' }) },
    { id: 'servers', label: t('servers.open'), icon: <Server aria-hidden />, go: () => navigate({ name: 'servers' }) },
    { id: 'browse', label: t('browse.open'), icon: <Package aria-hidden />, go: () => navigate({ name: 'browse', kind: sessionKind() }) },
  ];
  // Tab enters at the open place, or the first.
  const entry = Math.max(
    places.findIndex((p) => p.id === open),
    0,
  );

  const onKeyDown = (event: KeyboardEvent<HTMLElement>) => {
    const at = buttons.current.indexOf(document.activeElement as HTMLButtonElement);
    const next =
      event.key === 'ArrowRight' ? at + 1 : event.key === 'ArrowLeft' ? at - 1 : event.key === 'Home' ? 0 : event.key === 'End' ? places.length - 1 : null;
    if (next === null || at < 0) return;
    event.preventDefault();
    buttons.current[(next + places.length) % places.length]?.focus();
  };

  return (
    <nav aria-label={t('places.label')} className="tb-places" onKeyDown={onKeyDown}>
      {places.map((place, index) => (
        <button
          key={place.id}
          ref={(el) => {
            buttons.current[index] = el;
          }}
          type="button"
          className="tb-place"
          aria-current={place.id === open ? 'page' : undefined}
          tabIndex={index === entry ? 0 : -1}
          onClick={place.go}
        >
          {place.icon}
          {place.label}
        </button>
      ))}
    </nav>
  );
}
