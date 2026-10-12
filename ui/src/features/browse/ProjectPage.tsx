import type { BrowseRoute } from './Browse';

export function ProjectPage({ project }: { route: BrowseRoute; project: string }) {
  return <p>{project}</p>;
}
