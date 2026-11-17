import fs from 'node:fs';
import {pathToFileURL} from 'node:url';
import {loadApprovedSources, validateSourceNames} from './check-source-catalog.mjs';

const validDay = value => typeof value === 'string' && /^\d{4}-\d{2}-\d{2}$/.test(value) &&
  Number.isFinite(Date.parse(value+'T00:00:00Z')) && new Date(value+'T00:00:00Z').toISOString().slice(0,10) === value;

export function resolveSmokeProfile(profile, {
  approved = loadApprovedSources(),
  evidence = undefined,
  today = new Date().toISOString().slice(0,10),
} = {}) {
  validateSourceNames(approved, 'approved sources');
  if (profile === 'full') return {name:'full', expected_sources:approved, unverified_sources:[], approved_source_count:approved.length};
  if (profile !== 'cloud') throw Error(`Unknown smoke profile: ${profile}`);
  evidence ??= JSON.parse(fs.readFileSync('src-tauri/resources/source_verification_profiles.json', 'utf8'));
  if (evidence.version !== 1 || !validDay(evidence.review_after) || !validDay(today) ||
      !Array.isArray(evidence.manual_mac_sources)) throw Error('Invalid cloud profile evidence');
  const manual = evidence.manual_mac_sources;
  validateSourceNames(manual.map(row=>row.source), 'manual Mac sources');
  const critical = new Set(['數位發展部','國發會','國科會','經濟部']);
  for (const row of manual) {
    if (!approved.includes(row.source) || critical.has(row.source) || row.mac_status !== 'success' ||
        !row.mac_run_id || !row.host || !['official_verification_page','hosted_connection_timeout'].includes(row.reason) ||
        !validDay(row.checked_at) || row.checked_at > today || row.checked_at >= evidence.review_after ||
        Date.parse(evidence.review_after)-Date.parse(row.checked_at) > 45*86400000 ||
        !row.cloud_run?.startsWith('https://github.com/ECJura2000/taiwan-government-news-scraper/actions/runs/') ||
        !row.network_probe_run?.startsWith('https://github.com/ECJura2000/taiwan-government-news-scraper/actions/runs/')) {
      throw Error(`Invalid manual verification evidence: ${row.source}`);
    }
  }
  const manualNames = new Set(manual.map(row=>row.source));
  const expected = approved.filter(source=>!manualNames.has(source));
  if (!expected.length) throw Error('Cloud profile cannot omit every source');
  const expired = today >= evidence.review_after;
  return {name:'cloud', expected_sources:expired ? approved : expected, unverified_sources:expired ? [] : manual,
    approved_source_count:approved.length, review_after:evidence.review_after,
    evidence_status:expired ? 'expired' : 'current'};
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const profile = resolveSmokeProfile(process.argv[2] ?? 'cloud', {today:process.env.SOURCE_SMOKE_PROFILE_DATE});
  console.log(process.argv[3] === '--sources' ? profile.expected_sources.join('\n') : JSON.stringify(profile,null,2));
}
