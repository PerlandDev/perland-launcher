import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { ComingSoon } from './ComingSoon';
import { Dialog } from './Dialog';
import { ErrorCard } from './ErrorCard';
import { Menu } from './Menu';
import { Meter } from './Meter';
import { PlayButton } from './PlayButton';
import { Toggle } from './Toggle';

describe('Meter', () => {
  it('reports progress as a percentage and lights whole segments', () => {
    const { container } = render(<Meter value={0.5} label="Installing" />);
    expect(screen.getByRole('progressbar', { name: 'Installing' })).toHaveAttribute('aria-valuenow', '50');
    expect(container.querySelectorAll('[data-on]')).toHaveLength(10);
  });
});

describe('PlayButton', () => {
  it('plays when idle', async () => {
    const onPlay = vi.fn();
    render(<PlayButton state="idle" progress={null} onPlay={onPlay} onStop={vi.fn()} />);
    await userEvent.click(screen.getByRole('button', { name: 'Play' }));
    expect(onPlay).toHaveBeenCalledOnce();
  });

  it('shows the task and cancels while preparing', async () => {
    const onStop = vi.fn();
    render(
      <PlayButton
        state="preparing"
        progress={{ task: { key: 'install_minecraft', params: { version: '1.20.1' } }, done: 5, total: 10, bytes: 0 }}
        onPlay={vi.fn()}
        onStop={onStop}
      />,
    );
    const button = screen.getByRole('button', { name: 'Cancel: Installing Minecraft 1.20.1' });
    expect(button).toHaveTextContent('5 of 10 files');
    await userEvent.click(button);
    expect(onStop).toHaveBeenCalledOnce();
  });

  it('becomes Stop while running and explains when it is disabled', () => {
    const { rerender } = render(<PlayButton state="running" progress={null} onPlay={vi.fn()} onStop={vi.fn()} />);
    expect(screen.getByRole('button', { name: 'Stop' })).toBeEnabled();
    rerender(<PlayButton state="stopping" progress={null} onPlay={vi.fn()} onStop={vi.fn()} />);
    expect(screen.getByRole('button', { name: 'Stopping…' })).toBeDisabled();
    rerender(
      <PlayButton state="idle" progress={null} disabled disabledReason="Add an account to play" onPlay={vi.fn()} onStop={vi.fn()} />,
    );
    expect(screen.getByRole('button', { name: 'Play' })).toHaveAccessibleDescription('Add an account to play');
  });
});

describe('Toggle', () => {
  it('is a switch', async () => {
    const onChange = vi.fn();
    render(<Toggle checked={false} onChange={onChange} label="Minimize while playing" />);
    await userEvent.click(screen.getByRole('switch', { name: 'Minimize while playing' }));
    expect(onChange).toHaveBeenCalledWith(true);
  });

  it('when disabled stays focusable but ignores clicks and keys', async () => {
    const onChange = vi.fn();
    render(<Toggle checked={false} disabled onChange={onChange} label="Busy" />);
    const toggle = screen.getByRole('switch', { name: 'Busy' });
    expect(toggle).toHaveAttribute('aria-disabled', 'true');
    expect(toggle).toBeEnabled();
    toggle.focus();
    await userEvent.keyboard('{Enter} ');
    await userEvent.click(toggle);
    expect(onChange).not.toHaveBeenCalled();
    expect(toggle).toHaveFocus();
  });
});

describe('Dialog', () => {
  it('shows its title and closes on Escape', async () => {
    const onClose = vi.fn();
    render(
      <Dialog open title="Delete Pack?" onClose={onClose} actions={<button type="button">Delete</button>}>
        Body
      </Dialog>,
    );
    expect(screen.getByRole('dialog', { name: 'Delete Pack?' })).toBeInTheDocument();
    screen.getByRole('dialog').dispatchEvent(new Event('cancel', { cancelable: true }));
    expect(onClose).toHaveBeenCalled();
  });
});

describe('ErrorCard', () => {
  it('shows the translated message, the fix, and details on request', async () => {
    render(
      <ErrorCard
        error={{ category: 'disk', code: 'disk_io', message: 'x', fix: 'y', detail: 'permission denied', params: {} }}
      />,
    );
    expect(screen.getByText("Couldn't read or write a launcher file.")).toBeInTheDocument();
    expect(screen.queryByText('permission denied')).not.toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Details' }));
    expect(screen.getByText('permission denied')).toBeInTheDocument();
  });
});

describe('ComingSoon', () => {
  it('is visibly unavailable and not interactive', () => {
    render(<ComingSoon label="Paste a pack link" />);
    const tile = screen.getByText('Paste a pack link').closest('[aria-disabled]');
    expect(tile).toHaveAttribute('aria-disabled', 'true');
    expect(screen.queryByRole('button')).not.toBeInTheDocument();
    expect(screen.getByText('Coming soon')).toBeInTheDocument();
  });
});

describe('Menu', () => {
  it('opens, runs an item, and closes on Escape returning focus', async () => {
    const onSelect = vi.fn();
    render(<Menu label="More options" items={[{ label: 'Rename', onSelect }]} />);
    const trigger = screen.getByRole('button', { name: 'More options' });
    await userEvent.click(trigger);
    expect(screen.getByRole('menuitem', { name: 'Rename' })).toHaveFocus();
    await userEvent.keyboard('{Escape}');
    expect(screen.queryByRole('menu')).not.toBeInTheDocument();
    expect(trigger).toHaveFocus();
    await userEvent.click(trigger);
    await userEvent.click(screen.getByRole('menuitem', { name: 'Rename' }));
    expect(onSelect).toHaveBeenCalledOnce();
  });

  it('returns focus to the trigger after an item is chosen', async () => {
    render(<Menu label="More options" items={[{ label: 'Rename', onSelect: vi.fn() }]} />);
    const trigger = screen.getByRole('button', { name: 'More options' });
    await userEvent.click(trigger);
    await userEvent.click(screen.getByRole('menuitem', { name: 'Rename' }));
    expect(screen.queryByRole('menu')).not.toBeInTheDocument();
    expect(trigger).toHaveFocus();
  });

  it('closes when focus tabs out of the menu', async () => {
    render(<Menu label="More options" items={[{ label: 'Rename', onSelect: vi.fn() }]} />);
    await userEvent.click(screen.getByRole('button', { name: 'More options' }));
    expect(screen.getByRole('menu')).toBeInTheDocument();
    await userEvent.tab();
    expect(screen.queryByRole('menu')).not.toBeInTheDocument();
  });
});
