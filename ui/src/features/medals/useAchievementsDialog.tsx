import { useState } from 'react';
import { createPortal } from 'react-dom';
import type { ServerStats } from '../../ipc/schema';
import { AchievementsDialog } from './AchievementsDialog';

/** Open state for a server's achievements dialog. The dialog joins the page on the first open and stays, so closing it hands
 *  focus back natively; it is portalled out of any card, whose hover rules would light its Play behind the backdrop. */
export function useAchievementsDialog(server: ServerStats | undefined) {
  const [open, setOpen] = useState(false);
  const [opened, setOpened] = useState(false);
  const show = () => {
    setOpened(true);
    setOpen(true);
  };
  const dialog =
    server && opened ? createPortal(<AchievementsDialog server={server} open={open} onClose={() => setOpen(false)} />, document.body) : null;
  return { show, dialog };
}
