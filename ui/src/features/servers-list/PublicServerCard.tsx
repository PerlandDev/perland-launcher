import { useQueryClient } from '@tanstack/react-query';
import { useEffect, useId, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { PlotIcon } from '../../art/PlotIcon';
import { commands } from '../../ipc/commands';
import { isLauncherError } from '../../ipc/errors';
import type { PublicServerCard as Card } from '../../ipc/schema';
import { qk, useServerIcon } from '../../state/queries';
import { navigate } from '../../state/route';
import { pushError } from '../../state/toasts';
import { Button } from '../../ui/Button';
import { copyText } from '../../ui/copyText';
import { useOnScreen } from '../../ui/useOnScreen';
import { Check, Copy } from '../../ui/Icon';
import { cardIconSize, useServerIconSize } from '../join/iconScale';
import { NewsLine } from '../news/NewsLine';
import { InviteOnly } from '../servers/InviteOnly';
import { StatusPill } from '../servers/StatusPill';
import { gameLine, listStatusWord } from './labels';

/** One public server: icon, name, game line, status in words, its latest news, and Play. Play is raised at rest and lights
 *  only while its card is under the pointer or holds focus, so a grid of cards is never a wall of amber. */
export function PublicServerCard({ card }: { card: Card }) {
  const { t } = useTranslation();
  const nameId = useId();
  const icon = useServerIcon(card.token, card.iconVersion);
  // An icon that fails to decode gives the card the generated plot icon, as on Join server.
  const [failed, setFailed] = useState<string | null>(null);
  const src = icon.data && icon.data !== failed ? icon.data : null;
  const size = useServerIconSize(cardIconSize);
  const play = usePlay(card);
  const joinable = card.joinable && card.invite !== null;
  // News is asked for only while the card is on screen: the Engine allows 60 reads a minute per player, and a page holds up to 50 cards.
  const ref = useRef<HTMLElement>(null);
  const onScreen = useOnScreen(ref);
  return (
    <article ref={ref} className="server-tile" aria-labelledby={nameId}>
      <div className="server-tile-head">
        <span className="server-icon">
          {src ? (
            <img src={src} alt="" style={{ width: size, height: size }} draggable={false} onError={() => setFailed(src)} />
          ) : (
            <PlotIcon seed={card.name} size={size} />
          )}
        </span>
        <div className="grid min-w-0 content-start gap-1">
          <h3 id={nameId} className="server-tile-name">
            {card.name}
          </h3>
          <p className="tabular text-sm text-ink-dim">{gameLine(t, card)}</p>
          <span className="mt-1.5">
            <StatusPill status={card.status} players={card.players} gone={false} word={listStatusWord(t, card)} live={false} />
          </span>
        </div>
      </div>
      <NewsLine invite={card.invite} name={card.name} visible={onScreen} />
      {(card.whitelisted || (!joinable && card.address)) && (
        <div className="server-tile-facts">
          {card.whitelisted && <InviteOnly />}
          {!joinable && card.address && <JoinFromMinecraft address={card.address} />}
        </div>
      )}
      <Button
        size="lg"
        className="server-tile-play"
        aria-describedby={nameId}
        disabled={!joinable}
        aria-disabled={play.busy || undefined}
        onClick={play.run}
      >
        {play.busy ? t('servers.joining') : t('play.play')}
      </Button>
    </article>
  );
}

/** Play: add the server (or find the instance that already joins it), start the game with join, open its Home. */
function usePlay(card: Card) {
  const qc = useQueryClient();
  const [busy, setBusy] = useState(false);
  const running = useRef(false);
  const mounted = useRef(true);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);
  const run = () => {
    if (running.current || !card.invite) return;
    const invite = card.invite;
    running.current = true;
    setBusy(true);
    void (async () => {
      let id: string | null = null;
      try {
        // Always asked, even when the card names an instance: it answers the one that joins the server
        // now, so an instance deleted since the list loaded is never played.
        id = (await commands.joinAdd(invite)).id;
        if (id !== card.instance) await qc.invalidateQueries({ queryKey: qk.instances });
        await commands.play(id, true);
        navigate({ name: 'home', id });
      } catch (error) {
        // This server's instance is already running: its Home is where the player wants to be.
        if (id && isLauncherError(error) && error.code === 'already_running') navigate({ name: 'home', id });
        else pushError(error);
      } finally {
        running.current = false;
        if (mounted.current) setBusy(false);
      }
    })();
  };
  return { busy, run };
}

/** A server the launcher can't join yet: the address to type in Minecraft, with Copy. */
function JoinFromMinecraft({ address }: { address: string }) {
  const { t } = useTranslation();
  const [copied, setCopied] = useState(false);
  const timer = useRef<ReturnType<typeof setTimeout>>(undefined);
  const mounted = useRef(true);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
      clearTimeout(timer.current);
    };
  }, []);
  const copy = () =>
    void copyText(address).then((ok) => {
      if (!ok || !mounted.current) return;
      setCopied(true);
      clearTimeout(timer.current);
      timer.current = setTimeout(() => setCopied(false), 2000);
    });
  return (
    <div className="server-tile-address">
      <p className="text-sm">{t('servers.joinFromMinecraft', { address })}</p>
      <Button variant="ghost" icon={copied ? <Check /> : <Copy />} aria-label={copied ? t('common.copied') : t('server.copyAddress')} onClick={copy}>
        {copied ? t('common.copied') : t('common.copy')}
      </Button>
      <span className="sr-only" aria-live="polite">
        {copied ? t('server.addressCopied') : ''}
      </span>
    </div>
  );
}
