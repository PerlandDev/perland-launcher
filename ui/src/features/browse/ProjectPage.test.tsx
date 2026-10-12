import { screen, within } from '@testing-library/react';
import { expect, it } from 'vitest';
import projectFx from '../../ipc/contract/project_detail.json';
import type { ProjectDetail } from '../../ipc/schema';
import { routeSnapshot } from '../../state/route';
import { renderApp, sent } from '../../test/backend';
import { renderWithProviders } from '../../test/render';
import { Browse } from './Browse';

const route = { name: 'browse' as const, kind: 'mod' as const, project: 'sodium' };

it('shows the project, its gallery and its links', async () => {
  const { user, calls } = renderApp(<Browse route={route} />);
  expect(await screen.findByRole('heading', { name: 'Sodium', level: 1 })).toBeInTheDocument();
  expect(screen.getByText('by jellysquid3')).toBeInTheDocument();
  expect(screen.queryByText('The server needs it too')).not.toBeInTheDocument();
  await user.click(screen.getByRole('button', { name: 'Before and after' }));
  const dialog = await screen.findByRole('dialog');
  expect(within(dialog).getByRole('img')).toHaveAttribute('src', expect.stringMatching(/^data:image\//));
  await user.click(within(dialog).getByRole('button', { name: 'Close' }));
  await user.click(screen.getByRole('button', { name: 'Source code' }));
  expect(sent(calls, 'open_link')).toEqual([{ url: 'https://github.com/CaffeineMC/sodium' }]);
  await user.click(screen.getByRole('button', { name: 'Back to results' }));
  expect(routeSnapshot()).toEqual({ name: 'browse', kind: 'mod', instance: undefined });
});

it('renders the description without raw HTML or foreign images', async () => {
  const detail: ProjectDetail = {
    ...(projectFx as ProjectDetail),
    serverSide: 'required',
    body: [
      '# Sodium',
      'Text <b>bold</b> <script>alert(1)</script>',
      '[Wiki](https://example.com/wiki) [bad](javascript:alert(1)) [plain](http://example.com)',
      '![shot](https://cdn.modrinth.com/data/x/shot.png) ![tracker](https://i.imgur.com/x.png)',
    ].join('\n\n'),
  };
  const opened: unknown[] = [];
  const { user, container } = renderWithProviders(<Browse route={route} />, {
    ipc: (cmd, args) => {
      if (cmd === 'browse_project') return detail;
      if (cmd === 'browse_image') return 'data:image/png;base64,AAAA';
      if (cmd === 'browse_versions' || cmd === 'browse_installed') return [];
      if (cmd === 'instances_list') return { instances: [], damaged: [] };
      if (cmd === 'open_link') opened.push(args);
      return null;
    },
  });
  expect(await screen.findByText('The server needs it too')).toBeInTheDocument();
  const article = container.querySelector('.markdown')!;
  expect(article.querySelector('b, script')).toBeNull();
  expect(await within(article as HTMLElement).findByRole('img', { name: 'shot' })).toHaveAttribute('src', 'data:image/png;base64,AAAA');
  expect(within(article as HTMLElement).queryByRole('img', { name: 'tracker' })).not.toBeInTheDocument();
  expect(within(article as HTMLElement).getByText('tracker')).toBeInTheDocument();
  expect(within(article as HTMLElement).queryByRole('button', { name: 'bad' })).not.toBeInTheDocument();
  expect(within(article as HTMLElement).queryByRole('button', { name: 'plain' })).not.toBeInTheDocument();
  await user.click(within(article as HTMLElement).getByRole('button', { name: 'Wiki' }));
  expect(opened).toEqual([{ url: 'https://example.com/wiki' }]);
});
