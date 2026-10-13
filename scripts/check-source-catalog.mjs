import fs from 'node:fs';
import {pathToFileURL} from 'node:url';

const approvedPath = new URL('../src-tauri/resources/approved_sources.json', import.meta.url);
const catalogPath = new URL('../src-tauri/resources/sources.json', import.meta.url);

export function validateSourceNames(names, label = 'source names') {
  if (!Array.isArray(names) || names.length === 0 ||
      names.some(name => typeof name !== 'string' || !name || name !== name.trim()) ||
      new Set(names).size !== names.length) throw Error(`Invalid ${label}: expected a nonempty unique list of names`);
  return names;
}

export function loadApprovedSources() {
  return validateSourceNames(JSON.parse(fs.readFileSync(approvedPath, 'utf8')), 'approved source manifest');
}

export function matchesApprovedSources(actual, expected = loadApprovedSources()) {
  validateSourceNames(expected, 'expected sources');
  return Array.isArray(actual) && actual.length === expected.length &&
    new Set(actual).size === expected.length && actual.every(name => expected.includes(name));
}

export function checkSourceCatalog(listOutput = null) {
  const expected = loadApprovedSources();
  const catalog = JSON.parse(fs.readFileSync(catalogPath, 'utf8'));
  const names = validateSourceNames(Array.isArray(catalog) ? catalog.map(source => source?.name) : null, 'catalog');
  if (!matchesApprovedSources(names, expected)) throw Error('Catalog names do not match the approved source manifest');
  if (listOutput !== null) {
    const listed = listOutput.trim().split(/\r?\n/).map(name => name.trim());
    if (!matchesApprovedSources(listed, expected)) throw Error('CLI list-sources names do not match the approved source manifest');
  }
  return expected.length;
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  try {
    if (process.argv.length > 3) throw Error('Usage: node scripts/check-source-catalog.mjs [list-sources-output-file]');
    const output = process.argv[2] ? fs.readFileSync(process.argv[2], 'utf8') : null;
    console.log(checkSourceCatalog(output));
  } catch (error) {
    console.error(error.message);
    process.exitCode = 1;
  }
}
