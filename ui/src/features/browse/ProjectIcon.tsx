import { useState } from 'react';
import { PlotIcon } from '../../art/PlotIcon';
import { useBrowseImage } from '../../state/queries';

/** A project's own icon through the core's image cache, or a generated plot icon. */
export function ProjectIcon({ seed, url, size = 48 }: { seed: string; url: string | null; size?: number }) {
  const image = useBrowseImage(url, 'icon');
  const [failed, setFailed] = useState<string | null>(null);
  const src = image.data && image.data !== failed ? image.data : null;
  return (
    <span className="project-icon">
      {src ? (
        <img src={src} alt="" width={size} height={size} className="browse-icon" draggable={false} onError={() => setFailed(src)} />
      ) : (
        <PlotIcon seed={seed} size={size} />
      )}
    </span>
  );
}
