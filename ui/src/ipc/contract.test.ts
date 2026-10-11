import { describe, expect, it, vi } from 'vitest';
import type { ZodType } from 'zod';
import accounts from './contract/accounts.json';
import appInfo from './contract/app_info.json';
import errorFx from './contract/error.json';
import errorCodes from './contract/error_codes.json';
import events from './contract/events.json';
import instanceFx from './contract/instance.json';
import instanceListing from './contract/instance_listing.json';
import javaTest from './contract/java_test.json';
import loaderVersions from './contract/loader_versions.json';
import localMods from './contract/local_mods.json';
import manualProgress from './contract/manual_progress.json';
import mcVersions from './contract/mc_versions.json';
import newInstance from './contract/new_instance.json';
import openTargets from './contract/open_targets.json';
import packAddRequests from './contract/pack_add_requests.json';
import packArt from './contract/pack_art.json';
import packContents from './contract/pack_contents.json';
import packStatus from './contract/pack_status.json';
import runSnapshots from './contract/run_snapshots.json';
import settingsFx from './contract/settings.json';
import { decodeEvent } from './events';
import * as s from './schema';

const cases: [string, ZodType, unknown][] = [
  ['accounts', s.accountsFile, accounts],
  ['app_info', s.appInfo, appInfo],
  ['error', s.launcherError, errorFx],
  ['instance', s.instance, instanceFx],
  ['instance_listing', s.instanceListing, instanceListing],
  ['java_test', s.javaTest, javaTest],
  ['loader_versions', s.loaderVersion.array(), loaderVersions],
  ['local_mods', s.localMod.array(), localMods],
  ['manual_progress', s.manualProgress, manualProgress],
  ['mc_versions', s.mcVersion.array(), mcVersions],
  ['new_instance', s.newInstance, newInstance],
  ['open_targets', s.openTarget.array(), openTargets],
  ['pack_add_requests', s.packAddRequest.array(), packAddRequests],
  ['pack_art', s.packArtData, packArt],
  ['pack_contents', s.packContents, packContents],
  ['pack_status', s.packStatus.array(), packStatus],
  ['run_snapshots', s.runSnapshot.array(), runSnapshots],
  ['settings', s.settings, settingsFx],
  ['events', s.appEvent.array(), events],
];

describe('Rust contract fixtures', () => {
  it.each(cases)('%s matches its schema exactly', (_name, schema, fixture) => {
    const result = schema.safeParse(fixture);
    expect(result.success, JSON.stringify(result.error?.issues)).toBe(true);
  });

  it('lists error codes as snake_case strings', () => {
    expect(errorCodes.length).toBeGreaterThan(40);
    for (const code of errorCodes) expect(code).toMatch(/^[a-z0-9_]+$/);
  });

  it('decodes every event and rejects junk', () => {
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
    for (const e of events) expect(decodeEvent(e)).not.toBeNull();
    expect(decodeEvent({ type: 'nope' })).toBeNull();
    expect(decodeEvent({ type: 'log', instance: 'x' })).toBeNull();
    expect(warn).toHaveBeenCalledTimes(2);
  });
});
