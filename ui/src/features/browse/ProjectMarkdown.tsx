// A project's description. Raw HTML is skipped (no rehype-raw), images show only from the CDN
// through the core's image cache, and only https links open (in the system browser, from Rust).
import Markdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import { commands } from '../../ipc/commands';
import { useBrowseImage } from '../../state/queries';
import { pushError } from '../../state/toasts';

// A pure helper kept beside the component that uses it; losing fast refresh for this file is harmless.
// eslint-disable-next-line react-refresh/only-export-components
export function isCdnImage(src: string): boolean {
  try {
    const url = new URL(src);
    return url.protocol === 'https:' && url.host === 'cdn.modrinth.com' && !url.username && !url.password;
  } catch {
    return false;
  }
}

function MarkdownImage({ src, alt }: { src: string; alt: string }) {
  const image = useBrowseImage(src, 'gallery');
  return image.data ? <img src={image.data} alt={alt} className="md-img" /> : <span className="md-alt">{alt}</span>;
}

export function ProjectMarkdown({ source }: { source: string }) {
  return (
    <div className="markdown">
      <Markdown
        remarkPlugins={[remarkGfm]}
        skipHtml
        components={{
          // The page title is the h1; the description's headings start one level below it. The class and id
          // stay: remark-gfm's footnote label is an `sr-only` heading that must not become visible.
          h1: ({ children, className, id }) => (
            <h2 className={className} id={id}>
              {children}
            </h2>
          ),
          h2: ({ children, className, id }) => (
            <h3 className={className} id={id}>
              {children}
            </h3>
          ),
          h3: ({ children, className, id }) => (
            <h4 className={className} id={id}>
              {children}
            </h4>
          ),
          h4: ({ children, className, id }) => (
            <h5 className={className} id={id}>
              {children}
            </h5>
          ),
          h5: ({ children, className, id }) => (
            <h6 className={className} id={id}>
              {children}
            </h6>
          ),
          a: ({ href, children }) =>
            href?.startsWith('https://') ? (
              <button type="button" className="md-link" onClick={() => void commands.openLink(href).catch(pushError)}>
                {children}
              </button>
            ) : (
              <span>{children}</span>
            ),
          img: ({ src, alt }) =>
            typeof src === 'string' && isCdnImage(src) ? (
              <MarkdownImage src={src} alt={alt ?? ''} />
            ) : alt ? (
              <span className="md-alt">{alt}</span>
            ) : null,
        }}
      >
        {source}
      </Markdown>
    </div>
  );
}
