import { useId, type ReactNode } from 'react';
import { useTranslation } from 'react-i18next';
import { transitions as transitionsSchema, type Accent, type CustomRoom, type Theme } from '../../ipc/schema';
import { ACCENTS, FONTS, MADE_THEMES } from '../../state/appearance';
import { BACKGROUNDS, buttonsFor, deriveRoom, fitButtons, isHex, roomOk, type Swatch } from '../../state/customRoom';
import { useSettings } from '../../state/queries';
import { navigate } from '../../state/route';
import { Button } from '../../ui/Button';
import { Brush, WarningDiamond } from '../../ui/Icon';
import { PlayButton } from '../../ui/PlayButton';
import { ScreenFrame } from '../../ui/ScreenFrame';
import { useSettingsDraft } from '../settings/useSettingsDraft';

// Personalize (spec 2026-11-25): everything about the look, behind the brush in the title bar. The room, the button
// colour and the player's own room come first, then the text, the icons and the transitions Settings used to hold.

/** The rooms added in this version, which carry a New tag. */
const NEW_ROOMS: readonly Theme[] = ['nether', 'end', 'cherry'];
const TRANSITIONS = transitionsSchema.options;

/** One titled block of the screen. `radios` makes it a radio group, which names its options for assistive tech. */
function Section({ title, hint, radios, children }: { title: string; hint?: string; radios?: 'radiogroup' | 'group'; children: ReactNode }) {
  const id = useId();
  const Tag = radios ? 'fieldset' : 'section';
  const Head = radios ? 'legend' : 'h2';
  return (
    <Tag className="grid gap-3" role={radios ?? 'region'} aria-labelledby={id}>
      <div className="grid gap-1">
        <Head id={id} className="text-lg font-bold">
          {title}
        </Head>
        {hint && <p className="max-w-[65ch] text-sm text-ink-dim">{hint}</p>}
      </div>
      {children}
    </Tag>
  );
}

/** A segmented choice, as Settings has always drawn one. */
function Segments<T extends string>({ name, options, value, onChange, label }: { name: string; options: readonly T[]; value: T; onChange: (next: T) => void; label: (option: T) => ReactNode }) {
  return (
    <div className="flex flex-wrap gap-2">
      {options.map((option) => (
        <label key={option} className="seg" data-checked={value === option ? '' : undefined}>
          <input type="radio" name={name} className="sr-only" checked={value === option} onChange={() => onChange(option)} />
          {label(option)}
        </label>
      ))}
    </div>
  );
}

export function Personalize() {
  const { t } = useTranslation();
  const settings = useSettings();
  const { draft, edit } = useSettingsDraft();
  const data = draft ?? settings.data;
  if (!data) return null;
  const room = data.customRoom;
  const own = data.theme === 'custom';
  const name = (swatch: Swatch) => `${t(`personalize.hues.${swatch.hue}`)}, ${t(`personalize.tones.${swatch.tone}`)}`;
  // A new background keeps the buttons when they still read on it, else takes the nearest that do.
  const pickBackground = (background: string) => edit({ customRoom: { background, buttons: fitButtons(background, room.buttons) } });
  const pickButtons = (buttons: string) => edit({ customRoom: { ...room, buttons } });

  return (
    <ScreenFrame title={t('personalize.title')} subtitle={t('personalize.subtitle')} onBack={() => navigate({ name: 'home', id: null })}>
      <div className="grid max-w-3xl gap-10 pb-10">
        {/* The look applies at once, so the strip shows the choice being made rather than describing it. */}
        <div className="preview-strip">
          <div className="w-72 flex-none">
            <PlayButton state="idle" progress={null} onPlay={() => {}} onStop={() => {}} />
          </div>
          <Button>{t('personalize.previewButton')}</Button>
          <span className="flex items-center gap-2 text-sm font-semibold">
            <span className="h-2 w-2 flex-none bg-lime" aria-hidden />
            {t('personalize.previewRunning')}
          </span>
          <span className="text-sm text-ink-dim">{t('personalize.previewHint')}</span>
        </div>

        <Section title={t('personalize.room')} hint={t('personalize.roomHint')} radios="radiogroup">
          <div className="swatch-keys swatch-keys-wide">
            {MADE_THEMES.map((theme) => (
              <label key={theme} className="swatch-key" data-checked={data.theme === theme ? '' : undefined}>
                <input type="radio" name="room" className="sr-only" checked={data.theme === theme} onChange={() => edit({ theme })} />
                <span className="swatch" data-swatch={theme} aria-hidden>
                  <span className="swatch-face" />
                  <span className="swatch-lamp" />
                </span>
                <span className="swatch-name">
                  {t(`personalize.rooms.${theme}`)}
                  {/* The space keeps the key's name "Nether New" rather than one word. */}
                  {NEW_ROOMS.includes(theme) && <> <span className="new-tag">{t('personalize.new')}</span></>}
                </span>
              </label>
            ))}
          </div>
        </Section>

        {/* The player's own theme is not one of the made ones: one wide key below them makes it. It presses into its slot
            with the lime lamp while it is on, as a chosen theme does, and shows a sample of the colours it is mixed from,
            painted inline since it has no stylesheet block. */}
        <button type="button" className="swatch-key customize-key" aria-pressed={own} data-checked={own ? '' : undefined} onClick={() => edit({ theme: 'custom' })}>
          <Brush className="h-6 w-6 flex-none" aria-hidden />
          <span className="grid min-w-0 flex-1 gap-0.5 text-left">
            <span className="swatch-name">{t('personalize.customize')}</span>
            <span className="px-1 text-sm font-normal text-ink-dim">{t(own ? 'personalize.customizeOn' : 'personalize.customizeHint')}</span>
          </span>
          <span className="swatch customize-sample" style={ownSwatch(room)} aria-hidden>
            <span className="swatch-face" />
            <span className="swatch-lamp" />
          </span>
        </button>

        {own && (
          <Section title={t('personalize.yourColours')} hint={t('personalize.yourColoursHint')}>
            {!roomOk(room) && (
              <p className="flex items-start gap-2 text-sm font-semibold text-ink">
                <WarningDiamond className="mt-0.5 h-4 w-4 flex-none text-coral" aria-hidden />
                {t('personalize.oldColours')}
              </p>
            )}
            <Palette label={t('personalize.background')} name="room-background" swatches={BACKGROUNDS} value={room.background} onPick={pickBackground} nameOf={name} />
            <Palette label={t('personalize.buttons')} name="room-buttons" swatches={buttonsFor(room.background)} value={room.buttons} onPick={pickButtons} nameOf={name} />
          </Section>
        )}

        {/* The own room carries its own act hue, so the button colour is only offered beside a made room. */}
        {!own && (
          <Section title={t('personalize.accent')} hint={t('personalize.accentHint')} radios="radiogroup">
            <div className="swatch-keys swatch-keys-wide">
              {ACCENTS.map((accent) => (
                <label key={accent} className="swatch-key" data-checked={data.accent === accent ? '' : undefined}>
                  <input type="radio" name="accent" className="sr-only" checked={data.accent === accent} onChange={() => edit({ accent })} />
                  <span className="chip-face" data-chip={accent === 'room' ? undefined : (accent as Exclude<Accent, 'room'>)} data-swatch={accent === 'room' ? roomSwatch(data.theme) : undefined} aria-hidden />
                  <span className="swatch-name">{t(`personalize.accents.${accent}`)}</span>
                </label>
              ))}
            </div>
          </Section>
        )}

        <Section title={t('settings.font')} radios="group">
          <Segments
            name="font"
            options={FONTS}
            value={data.font}
            onChange={(font) => edit({ font })}
            label={(font) => <span className={font === 'system' ? 'font-system' : 'font-hyperlegible'}>{t(font === 'system' ? 'settings.fontSystem' : 'settings.fontHyperlegible')}</span>}
          />
        </Section>

        <Section title={t('settings.iconShape')} radios="group">
          <Segments
            name="icon-shape"
            options={['square', 'round'] as const}
            value={data.iconShape}
            onChange={(iconShape) => edit({ iconShape })}
            label={(shape) => t(shape === 'square' ? 'settings.iconSquare' : 'settings.iconRound')}
          />
        </Section>

        <Section title={t('settings.transitions')} radios="group">
          <Segments name="transitions" options={TRANSITIONS} value={data.transitions} onChange={(transitions) => edit({ transitions })} label={(style) => t(`settings.transitionsOptions.${style}`)} />
        </Section>
      </div>
    </ScreenFrame>
  );
}

/** One palette of the own theme: a radio group of colour chips, the chosen one pressed with the lime lamp. Each chip is
 *  named by its hue and tone, so a screen reader says more than a colour code. */
function Palette({ label, name, swatches, value, onPick, nameOf }: { label: string; name: string; swatches: readonly Swatch[]; value: string; onPick: (value: string) => void; nameOf: (swatch: Swatch) => string }) {
  const id = useId();
  return (
    <div role="radiogroup" aria-labelledby={id} className="grid gap-2">
      <span id={id} className="text-sm font-semibold">
        {label}
      </span>
      <div className="flex flex-wrap gap-2">
        {swatches.map((swatch) => (
          <label key={swatch.value} className="colour-chip" title={nameOf(swatch)} data-checked={value === swatch.value ? '' : undefined}>
            <input type="radio" name={name} className="sr-only" aria-label={nameOf(swatch)} checked={value === swatch.value} onChange={() => onPick(swatch.value)} />
            <span className="colour-chip-face" style={{ background: swatch.value }} aria-hidden />
          </label>
        ))}
      </div>
    </div>
  );
}

/** A made room to paint a sample with; the own room borrows the default's, since its own sample is inline. */
const roomSwatch = (theme: Theme): Theme => (theme === 'custom' ? 'laminate' : theme);

/** The own room's sample, painted from the two colours rather than from a stylesheet block: the same tokens a
 *  `[data-swatch]` block sets, on the sample itself. */
function ownSwatch(room: CustomRoom): Record<string, string> {
  const safe = isHex(room.background) && isHex(room.buttons) ? room : { background: '#101820', buttons: '#ff6ad5' };
  return Object.fromEntries(Object.entries(deriveRoom(safe)).map(([name, value]) => [`--color-${name}`, value]));
}
