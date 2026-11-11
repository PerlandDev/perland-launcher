import { useQuery } from '@tanstack/react-query';
import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { commands } from '../../ipc/commands';
import { isLauncherError } from '../../ipc/errors';
import type { ContentKind, Instance } from '../../ipc/schema';
import { useAccounts, useInstances, usePlayerSkin, useServersList } from '../../state/queries';
import { navigate } from '../../state/route';
import { useRun } from '../../state/runStore';
import { pushError } from '../../state/toasts';
import { Button } from '../../ui/Button';
import { ChevronRight, Play, Plus, Search, Server, Shirt, Trophy } from '../../ui/Icon';
import { useSingleFlight } from '../../ui/useSingleFlight';
import { pick } from '../home/pick';
import { MedalBadge } from '../medals/MedalBadge';
import { useMedalCase } from '../medals/useMedalCase';
import { PlayerFigure } from '../player/PlayerFigure';
import { gameLine, listStatusWord } from '../servers-list/labels';
import { NO_FILTERS } from '../servers-list/session';
import { StatusPill } from '../servers/StatusPill';

const KINDS: readonly ContentKind[] = ['modpack', 'mod', 'resource_pack', 'shader'];
/** How many servers the Lobby shows: a taste of the list, never the list. */
const BUSIEST = 3;

/** The way back into the last instance: its one lit Play while it can start, a raised Open otherwise (no account to play
 *  with, or it is already starting or running). Either way the player lands on its Home, where the progress shows. */
function Resume({ instance }: { instance: Instance }) {
  const { t } = useTranslation();
  const run = useRun(instance.id);
  const accounts = useAccounts();
  const playOnce = useSingleFlight();
  const canPlay = Boolean(accounts.data?.active) && run.state === 'idle';
  const go = () => {
    navigate({ name: 'home', id: instance.id });
    if (canPlay) playOnce((done) => void commands.play(instance.id).catch(pushError).finally(done));
  };
  return (
    <div className="lobby-resume">
      <p className="min-w-0 text-right">
        <span className="block text-sm text-ink-dim">{t('lobby.last')}</span>
        <span className="block truncate font-bold">{instance.name}</span>
      </p>
      {canPlay ? (
        <Button variant="primary" size="lg" icon={<Play aria-hidden />} aria-label={t('lobby.playLabel', { name: instance.name })} onClick={go}>
          {t('play.play')}
        </Button>
      ) : (
        <Button size="lg" aria-label={t('lobby.openLabel', { name: instance.name })} onClick={go}>
          {t('lobby.open')}
        </Button>
      )}
    </div>
  );
}

function SkinsPanel() {
  const { t } = useTranslation();
  const skin = usePlayerSkin();
  // The catalog is the Skins screen's own query, so a visit there finds it warm.
  const catalog = useQuery({ queryKey: ['skinCatalog'], queryFn: commands.skinCatalog });
  const looks = catalog.data?.manifest?.looks.length ?? 0;
  const wardrobe = skin.data?.wardrobe;
  const [wave, setWave] = useState(false);
  const open = () => navigate({ name: 'skins' });
  return (
    <article className="panel lobby-skins" aria-labelledby="lobby-skins">
      {/* The figure is the picture, as in the fitting room. A click on it is the key below, for the mouse only. */}
      <div className="well lobby-stage" onClick={open} onPointerEnter={() => setWave(true)} onPointerLeave={() => setWave(false)}>
        <PlayerFigure wave={wave} />
      </div>
      <div className="grid gap-1">
        <h2 id="lobby-skins" className="lobby-title">
          <Shirt aria-hidden />
          {t('titlebar.skins')}
        </h2>
        <p className="text-sm text-ink-dim">
          {wardrobe !== undefined && (wardrobe > 0 ? t('lobby.skins.wardrobe', { count: wardrobe }) : t('lobby.skins.empty'))}
          {looks > 0 && ` ${t('lobby.skins.looks', { count: looks })}`}
        </p>
      </div>
      <div>
        <Button icon={<Shirt aria-hidden />} onClick={open} onFocus={() => setWave(true)} onBlur={() => setWave(false)}>
          {t('lobby.skins.open')}
        </Button>
      </div>
    </article>
  );
}

function ServersPanel() {
  const { t } = useTranslation();
  const list = useServersList({ ...NO_FILTERS, offset: 0 });
  const busiest = (list.data?.pages[0]?.servers ?? []).slice(0, BUSIEST);
  // Off at the Engine is a calm word, never an error; so is a list that can't be reached from here.
  const closed = list.isError && isLauncherError(list.error) && list.error.code === 'server_list_closed';
  const open = () => navigate({ name: 'servers' });
  return (
    <article className="panel lobby-servers" aria-labelledby="lobby-servers">
      <div className="flex items-baseline justify-between gap-4">
        <h2 id="lobby-servers" className="lobby-title">
          <Server aria-hidden />
          {t('servers.open')}
        </h2>
        {busiest.length > 0 && <p className="text-sm text-ink-dim">{t('lobby.servers.busiest')}</p>}
      </div>
      {list.isError ? (
        <p className="lobby-note">{closed ? t('lobby.servers.closed') : t('lobby.servers.unavailable')}</p>
      ) : list.isSuccess && busiest.length === 0 ? (
        <p className="lobby-note">{t('lobby.servers.none')}</p>
      ) : (
        <ul className="lobby-rows">
          {busiest.map((card) => {
            const status = listStatusWord(t, card);
            return (
              <li key={card.token}>
                <button type="button" className="lobby-row" aria-label={t('lobby.servers.row', { name: card.displayName, status })} onClick={open}>
                  <span className="min-w-0 flex-1">
                    <span className="block truncate font-bold">{card.displayName}</span>
                    <span className="block truncate text-sm text-ink-dim">{gameLine(t, card)}</span>
                  </span>
                  <StatusPill status={card.status} players={card.players} gone={false} word={status} live={false} />
                  <ChevronRight className="lobby-chevron" aria-hidden />
                </button>
              </li>
            );
          })}
        </ul>
      )}
      <div className="flex flex-wrap gap-3">
        <Button icon={<Server aria-hidden />} onClick={open}>
          {t('lobby.servers.all')}
        </Button>
        <Button variant="ghost" onClick={open}>
          {t('freeServer.entry.create')}
        </Button>
      </div>
    </article>
  );
}

function BrowsePanel() {
  const { t } = useTranslation();
  return (
    <article className="panel lobby-browse" aria-labelledby="lobby-browse">
      <div className="grid gap-1">
        <h2 id="lobby-browse" className="lobby-title">
          <Search aria-hidden />
          {t('browse.open')}
        </h2>
        <p className="text-sm text-ink-dim">{t('lobby.browse.hint')}</p>
      </div>
      <div className="lobby-kinds">
        {KINDS.map((kind) => (
          <Button key={kind} onClick={() => navigate({ name: 'browse', kind })}>
            {t(`browse.kinds.${kind}`)}
          </Button>
        ))}
      </div>
    </article>
  );
}

function MedalsPanel() {
  const { t } = useTranslation();
  const { list, earned } = useMedalCase();
  return (
    <article className="panel lobby-medals" aria-labelledby="lobby-medals">
      <div className="flex items-baseline justify-between gap-4">
        <h2 id="lobby-medals" className="lobby-title">
          <Trophy aria-hidden />
          {t('titlebar.achievements')}
        </h2>
        <p className="tabular text-sm text-ink-dim">{t('medals.earnedCount', { earned, total: list.length })}</p>
      </div>
      {/* The medals themselves: earned ones in their metal, the rest as empty slots. The whole case is one key. */}
      <button type="button" className="lobby-case" aria-label={t('lobby.medals.open')} onClick={() => navigate({ name: 'achievements' })}>
        {list.map((medal) => (
          <MedalBadge key={medal.id} id={medal.id} tier={medal.tier} size={40} />
        ))}
      </button>
    </article>
  );
}

/** The Lobby ("Home" in the rail): everything the launcher offers on one screen, each as the real thing (the player's own
 *  figure, the busiest servers, the medals earned), never as an icon and a blurb. Its one lit key plays the last instance. */
export function Lobby() {
  const { t } = useTranslation();
  const accounts = useAccounts();
  const active = accounts.data?.accounts.find((a) => a.uuid === accounts.data?.active) ?? null;
  const last = pick(useInstances().data, null);
  return (
    <section className="lobby" aria-labelledby="lobby-title">
      <header className="lobby-head">
        <div className="min-w-0">
          <h1 id="lobby-title" className="truncate font-display text-[2rem] leading-none">
            {active ? t('lobby.hello', { name: active.username }) : t('lobby.welcome')}
          </h1>
          <p className="mt-2 text-ink-dim">{t('lobby.subtitle')}</p>
        </div>
        {last.kind === 'instance' && <Resume instance={last.instance} />}
        {last.kind === 'empty' && (
          <Button variant="primary" size="lg" icon={<Plus aria-hidden />} onClick={() => navigate({ name: 'newInstance' })}>
            {t('rail.newInstance')}
          </Button>
        )}
      </header>
      <div className="lobby-scroll">
        <div className="lobby-grid">
          <SkinsPanel />
          <ServersPanel />
          <BrowsePanel />
          <MedalsPanel />
        </div>
      </div>
    </section>
  );
}
