import { useState } from 'react';
import type { Instance } from '../ipc/schema';
import { useInstanceArt, useSettings } from '../state/queries';
import { artSeed } from './seed';
import { InstanceIcon } from './InstanceIcon';
import { pixelUpscale } from './upscale';

/** An instance's icon at a small size, as the rail draws it: its own picture (a pack's, or the player's upload) when it
 *  has one, else its generated plate, in the shape Personalize gives the rail's icons. Decoration: the words beside it
 *  name the instance. */
export function InstanceThumb({ instance, size = 20 }: { instance: Instance; size?: number }) {
  const icon = useInstanceArt(instance.id).data?.icon ?? null;
  const shape = useSettings().data?.iconShape ?? 'square';
  const [failed, setFailed] = useState<string | null>(null);
  const [upscaled, setUpscaled] = useState<string | null>(null);
  return (
    <span className="instance-thumb" data-shape={shape} style={{ width: size, height: size }} aria-hidden>
      {icon && failed !== icon ? (
        <img
          src={icon}
          alt=""
          width={size}
          height={size}
          draggable={false}
          data-upscale={upscaled === icon ? '' : undefined}
          onLoad={(e) => {
            const { naturalWidth: width, naturalHeight: height } = e.currentTarget;
            setUpscaled(pixelUpscale({ width, height }, { width: size, height: size }) ? icon : null);
          }}
          onError={() => setFailed(icon)}
        />
      ) : (
        <InstanceIcon name={instance.name} seed={artSeed(instance)} size={size} />
      )}
    </span>
  );
}
