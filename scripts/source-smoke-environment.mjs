import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import {execFileSync} from 'node:child_process';

const version = (programs, args = ['--version']) => {
  for (const program of programs) {
    try {
      return {available:true, program,
        version:execFileSync(program, args, {encoding:'utf8',timeout:5000,stdio:['ignore','pipe','ignore']}).trim()};
    } catch { /* Never print environment variables or command stderr. */ }
  }
  return {available:false, programs};
};
const output = process.argv[2];
if (!output) throw Error('Usage: source-smoke-environment.mjs OUTPUT_JSON');
const evidence = {version:1, platform:process.platform, architecture:process.arch,
  os_release:os.release(), os_version:os.version(), node_version:process.version,
  linux_distribution:fs.existsSync('/etc/os-release') ? fs.readFileSync('/etc/os-release','utf8').trim() : null,
  chrome:version(['google-chrome','google-chrome-stable','chromium','chromium-browser']),
  openssl:version(['openssl'], ['version','-a']), curl:version(['curl'])};
fs.mkdirSync(path.dirname(output), {recursive:true});
fs.writeFileSync(output, JSON.stringify(evidence,null,2)+'\n');
console.log(JSON.stringify(evidence));
