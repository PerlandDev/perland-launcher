import { describe, expect, it, vi } from 'vitest';
import type { ZodType } from 'zod';
import accounts from './contract/accounts.json';
import artChoices from './contract/art_choices.json';
import appInfo from './contract/app_info.json';
import contentFiles from './contract/content_files.json';
import deepLinksFx from './contract/deep_links.json';
import emailCodeSent from './contract/email_code_sent.json';
import errorFx from './contract/error.json';
import errorCodes from './contract/error_codes.json';
import events from './contract/events.json';
import freeServerPage from './contract/free_server_page.json';
import iconChoices from './contract/icon_choices.json';
import importPreview from './contract/import_preview.json';
import importRequest from './contract/import_request.json';
import installPlans from './contract/install_plans.json';
import installResult from './contract/install_result.json';
import inviteLinksFx from './contract/invite_links.json';
import instanceArt from './contract/instance_art.json';
import instanceFx from './contract/instance.json';
import instanceListing from './contract/instance_listing.json';
import javaOverview from './contract/java_overview.json';
import javaTest from './contract/java_test.json';
import joinPreviewFx from './contract/join_preview.json';
import linkCode from './contract/link_code.json';
import listQueries from './contract/list_queries.json';
import loaderVersions from './contract/loader_versions.json';
import localMods from './contract/local_mods.json';
import manualProgress from './contract/manual_progress.json';
import mcVersions from './contract/mc_versions.json';
import newInstance from './contract/new_instance.json';
import openTargets from './contract/open_targets.json';
import packAddRequests from './contract/pack_add_requests.json';
import packContents from './contract/pack_contents.json';
import packStatus from './contract/pack_status.json';
import perlandStats from './contract/perland_stats.json';
import perlandStatus from './contract/perland_status.json';
import planRequests from './contract/plan_requests.json';
import playtimeFx from './contract/playtime.json';
import continueTargetFx from './contract/continue_target.json';
import projectDetail from './contract/project_detail.json';
import publicServerPage from './contract/public_server_page.json';
import restorePoints from './contract/restore_points.json';
import runSnapshots from './contract/run_snapshots.json';
import searchPage from './contract/search_page.json';
import searchRequests from './contract/search_requests.json';
import serverInstanceFx from './contract/server_instance.json';
import serverNews from './contract/server_news.json';
import serverStatusFx from './contract/server_status.json';
import settingsFx from './contract/settings.json';
import shareChoiceFx from './contract/share_choice.json';
import sharePreviewFx from './contract/share_preview.json';
import updateStates from './contract/update_states.json';
import versions from './contract/versions.json';
import yourServersPage from './contract/your_servers_page.json';
import { decodeEvent } from './events';
import * as s from './schema';

const cases: [string, ZodType, unknown][] = [
  ['accounts', s.accountsFile, accounts],
  ['app_info', s.appInfo, appInfo],
  ['content_files', s.contentFile.array(), contentFiles],
  ['error', s.launcherError, errorFx],
  ['instance', s.instance, instanceFx],
  ['instance_listing', s.instanceListing, instanceListing],
  ['java_test', s.javaFound.array(), javaTest],
  ['java_overview', s.javaOverview.array(), javaOverview],
  ['loader_versions', s.loaderVersion.array(), loaderVersions],
  ['local_mods', s.localMod.array(), localMods],
  ['manual_progress', s.manualProgress, manualProgress],
  ['mc_versions', s.mcVersion.array(), mcVersions],
  ['new_instance', s.newInstance, newInstance],
  ['import_preview', s.importPreview, importPreview],
  ['share_preview', s.sharePreview, sharePreviewFx],
  ['share_choice', s.shareChoice, shareChoiceFx],
  ['import_request', s.importRequest, importRequest],
  ['open_targets', s.openTarget.array(), openTargets],
  ['pack_add_requests', s.packAddRequest.array(), packAddRequests],
  ['instance_art', s.instanceArtData, instanceArt],
  ['art_choices', s.artChoice.array(), artChoices],
  ['icon_choices', s.iconChoice.array(), iconChoices],
  ['pack_contents', s.packContents, packContents],
  ['pack_status', s.packStatus.array(), packStatus],
  ['playtime', s.playtimeEntry.array(), playtimeFx],
  ['continue_target', s.continueTarget.nullable().array(), continueTargetFx],
  ['run_snapshots', s.runSnapshot.array(), runSnapshots],
  ['settings', s.settings, settingsFx],
  ['server_instance', s.instance, serverInstanceFx],
  ['restore_points', s.restorePoint.nullable().array(), restorePoints],
  ['join_preview', s.joinPreview.array(), joinPreviewFx],
  ['server_status', s.serverStatusView.array(), serverStatusFx],
  ['public_server_page', s.publicServerPage, publicServerPage],
  ['server_news', s.serverNews, serverNews],
  ['perland_status', s.perlandStatus.array(), perlandStatus],
  ['link_code', s.linkCode, linkCode],
  ['perland_stats', s.perlandStats.nullable().array(), perlandStats],
  ['email_code_sent', s.emailCodeSent, emailCodeSent],
  ['your_servers_page', s.yourServersPage, yourServersPage],
  ['free_server_page', s.freeServerPage.array(), freeServerPage],
  ['deep_links', s.deepLink.array(), deepLinksFx],
  ['invite_links', s.inviteLinkCase.array(), inviteLinksFx],
  ['update_states', s.updateState.array(), updateStates],
  ['events', s.appEvent.array(), events],
  ['search_page', s.searchPage, searchPage],
  ['project_detail', s.projectDetail, projectDetail],
  ['versions', s.versionInfo.array(), versions],
  ['install_plans', s.installPlanView.array(), installPlans],
  ['install_result', s.installResult, installResult],
  ['search_requests', s.searchRequest.array(), searchRequests],
  ['list_queries', s.listQuery.array(), listQueries],
  ['plan_requests', s.browsePlanRequest.array(), planRequests],
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
