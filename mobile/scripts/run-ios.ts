// `expo run:ios` on this Mac: Nix's compiler and SDK variables fight Xcode's toolchain, Expo's
// stub script calls clang through the /usr/bin shim, CocoaPods wants a UTF-8 locale, and the
// build opens more files than the default limit. Extra arguments go to expo, e.g. --device.

import { spawn } from 'node:child_process';
import { join } from 'node:path';

const root = join(import.meta.dir, '..');
const env = { ...process.env };
for (const key of ['AR', 'AS', 'CC', 'CPATH', 'CPLUS_INCLUDE_PATH', 'CXX', 'C_INCLUDE_PATH', 'LD', 'LIBRARY_PATH', 'NM', 'RANLIB', 'SDKROOT', 'STRIP']) {
  delete env[key];
}
env.DEVELOPER_DIR = '/Applications/Xcode.app/Contents/Developer';
env.LANG = 'en_US.UTF-8';
env.LC_ALL = 'en_US.UTF-8';
env.PATH = ['/usr/bin', ...(env.PATH ?? '').split(':').filter((p) => p && !p.startsWith('/nix/store/')), '/bin', '/usr/sbin', '/sbin']
  .filter((p, i, all) => all.indexOf(p) === i)
  .join(':');

const args = process.argv.slice(2);
const device = args.includes('--device') ? [] : ['--device', 'formalnotes-iphone'];
const child = spawn('/bin/sh', ['-c', 'ulimit -n 65536 && exec "$0" "$@"', 'bunx', 'expo', 'run:ios', ...device, ...args], {
  cwd: root,
  env,
  stdio: 'inherit',
});
child.on('exit', (code, signal) => {
  if (signal) process.kill(process.pid, signal);
  else process.exit(code ?? 1);
});
