import { useRef, type KeyboardEvent, type ReactNode } from 'react';
import { useTranslation } from 'react-i18next';
import { usePlayerSkin } from '../../state/queries';
import { navigate, useRoute, type Route } from '../../state/route';
import { Home, Search, Server, Shirt, Trophy } from '../../ui/Icon';
import { sessionKind } from '../browse/session';
import { useMedalCase } from '../medals/useMedalCase';

interface Place {
  id: string;
  label: string;
  /** What the place holds, shown under its name while the rail is wide. */
  word: string | null;
  icon: ReactNode;
  open: boolean;
  go: () => void;
}

/** Which place a route belongs to, so its row stays marked on the screens it leads to. */
function placeOf(route: Route): string | null {
  switch (route.name) {
    case 'lobby':
      return 'home';
    case 'skins':
      return 'skins';
    case 'servers':
      return 'servers';
    case 'browse':
      // Browse opened from an instance's Mods & packs belongs to that instance.
      return route.instance ? null : 'browse';
    case 'achievements':
      return 'achievements';
    default:
      return null;
  }
}

/** The launcher's places, by name, at the top of the rail: Home (the Lobby), Skins, Servers, Browse and Achievements.
 *  Narrow they are icons named by their tooltip; wide each says its name and what it holds. They are flat rows, never
 *  cabinets (those are instances) and never lit; the open one stands on its step beside the coin-slot light. One Tab
 *  stop, and the arrow keys walk them, as along the cabinets. */
export function RailPlaces({ wide }: { wide: boolean }) {
  const { t } = useTranslation();
  const route = useRoute();
  const skin = usePlayerSkin();
  const { list, earned } = useMedalCase();
  const buttons = useRef<(HTMLButtonElement | null)[]>([]);
  const open = placeOf(route);
  const wardrobe = skin.data?.wardrobe;
  const places: Place[] = [
    { id: 'home', label: t('places.home'), word: null, icon: <Home aria-hidden />, open: open === 'home', go: () => navigate({ name: 'lobby' }) },
    {
      id: 'skins',
      label: t('titlebar.skins'),
      // No count until it is known; an empty wardrobe invites instead of counting nothing.
      word: wardrobe === undefined ? null : wardrobe > 0 ? t('places.skinsWord', { count: wardrobe }) : t('places.skinsEmpty'),
      icon: <Shirt aria-hidden />,
      open: open === 'skins',
      go: () => navigate({ name: 'skins' }),
    },
    { id: 'servers', label: t('servers.open'), word: t('places.serversWord'), icon: <Server aria-hidden />, open: open === 'servers', go: () => navigate({ name: 'servers' }) },
    {
      id: 'browse',
      label: t('browse.open'),
      word: t('places.browseWord'),
      icon: <Search aria-hidden />,
      open: open === 'browse',
      go: () => navigate({ name: 'browse', kind: sessionKind() }),
    },
    {
      id: 'achievements',
      label: t('titlebar.achievements'),
      word: t('places.medalsWord', { earned, total: list.length }),
      icon: <Trophy aria-hidden />,
      open: open === 'achievements',
      go: () => navigate({ name: 'achievements' }),
    },
  ];
  // Tab enters at the open place, or the first.
  const entry = Math.max(
    places.findIndex((p) => p.open),
    0,
  );

  const onKeyDown = (event: KeyboardEvent<HTMLUListElement>) => {
    const at = buttons.current.indexOf(document.activeElement as HTMLButtonElement);
    const next =
      event.key === 'ArrowDown' ? at + 1 : event.key === 'ArrowUp' ? at - 1 : event.key === 'Home' ? 0 : event.key === 'End' ? places.length - 1 : null;
    if (next === null || at < 0) return;
    event.preventDefault();
    buttons.current[(next + places.length) % places.length]?.focus();
  };

  return (
    <nav aria-label={t('places.label')} className="rail-places">
      <ul onKeyDown={onKeyDown}>
        {places.map((place, index) => (
          <li key={place.id}>
            <button
              ref={(el) => {
                buttons.current[index] = el;
              }}
              type="button"
              className="rail-place"
              data-selected={place.open ? '' : undefined}
              aria-current={place.open ? 'page' : undefined}
              aria-label={place.label}
              // Narrow, the name is nowhere on screen, so the tooltip carries it.
              title={wide ? undefined : place.label}
              tabIndex={index === entry ? 0 : -1}
              onClick={place.go}
            >
              <span className="rail-place-icon">{place.icon}</span>
              {wide && (
                <span className="rail-text">
                  <span className="rail-name">{place.label}</span>
                  {place.word && <span className="rail-word">{place.word}</span>}
                </span>
              )}
            </button>
          </li>
        ))}
      </ul>
    </nav>
  );
}
