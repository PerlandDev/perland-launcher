/// <reference types="vite/client" />

/** The release notes this build carries, as scripts/release/cli.mjs `notes` writes them (vite.config.ts). */
declare module 'virtual:release-notes' {
  const notes: unknown;
  export default notes;
}
