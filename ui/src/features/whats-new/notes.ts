import raw from 'virtual:release-notes';
import { releaseNotes, type ReleaseNotes } from '../../ipc/schema';

// The release notes this build carries (ui/releaseNotesPlugin.mjs): a release's are worked out by the Release
// workflow; a dev build's come from the checkout, with unreleased notes under no version.
const parsed = releaseNotes.safeParse(raw);
if (!parsed.success) console.error("What's new: the bundled release notes are not in the expected shape", parsed.error);

/** Empty when the bundled file is not what this build expects: the launcher opens without its notes rather than not at all. */
export const releaseNotesData: ReleaseNotes = parsed.success ? parsed.data : { versions: [], groups: [] };
