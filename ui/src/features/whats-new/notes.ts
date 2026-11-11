import raw from 'virtual:release-notes';
import { releaseNotes, type ReleaseNotes } from '../../ipc/schema';

/** The release notes this build carries (ui/vite.config.ts): a release's are worked out by the Release
 *  workflow; a dev build's come from the checkout, with unreleased notes under no version. */
export const releaseNotesData: ReleaseNotes = releaseNotes.parse(raw);
