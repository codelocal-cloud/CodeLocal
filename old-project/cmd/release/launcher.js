#!/usr/bin/env node
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { spawnSync } = require('node:child_process');

const DEFAULT_REGISTRY = 'https://registry.npmjs.org';
const UPDATE_CACHE_TTL_MS = 60 * 60 * 1000;
const UPDATE_TIMEOUT_MS = 1800;
const FALSE_VALUES = new Set(['0', 'false', 'off', 'no']);
const CHANNEL_RE = /^[A-Za-z][A-Za-z0-9._-]{0,31}$/;
const SEMVER_RE = /^(\d+)\.(\d+)\.(\d+)(?:-([0-9A-Za-z.-]+))?(?:\+[0-9A-Za-z.-]+)?$/;

function normalizeVersion(value) {
  const text = String(value || '').trim();
  if (/^[vV]\d/.test(text)) return text.slice(1);
  return text;
}

function parseVersion(value) {
  const match = SEMVER_RE.exec(normalizeVersion(value));
  if (!match) return null;
  return {
    major: Number(match[1]),
    minor: Number(match[2]),
    patch: Number(match[3]),
    prerelease: match[4] ? match[4].split('.') : [],
  };
}

function comparePrerelease(left, right) {
  if (left.length === 0 && right.length === 0) return 0;
  if (left.length === 0) return 1;
  if (right.length === 0) return -1;
  const limit = Math.max(left.length, right.length);
  for (let index = 0; index < limit; index += 1) {
    if (index >= left.length) return -1;
    if (index >= right.length) return 1;
    if (left[index] === right[index]) continue;
    const leftNumeric = /^\d+$/.test(left[index]);
    const rightNumeric = /^\d+$/.test(right[index]);
    if (leftNumeric && rightNumeric) {
      const a = BigInt(left[index]);
      const b = BigInt(right[index]);
      return a < b ? -1 : 1;
    }
    if (leftNumeric !== rightNumeric) return leftNumeric ? -1 : 1;
    return left[index] < right[index] ? -1 : 1;
  }
  return 0;
}

function compareVersions(left, right) {
  const a = parseVersion(left);
  const b = parseVersion(right);
  if (!a || !b) return null;
  for (const field of ['major', 'minor', 'patch']) {
    if (a[field] !== b[field]) return a[field] < b[field] ? -1 : 1;
  }
  return comparePrerelease(a.prerelease, b.prerelease);
}

function falseLike(value) {
  return FALSE_VALUES.has(String(value || '').trim().toLowerCase());
}

function releaseChannel(installedVersion, env = process.env) {
  const configured = String(env.CODELOCAL_RELEASE_CHANNEL || '').trim();
  if (CHANNEL_RE.test(configured)) return configured;
  const parsed = parseVersion(installedVersion);
  return parsed && parsed.prerelease.length > 0 ? 'beta' : 'latest';
}

function updateCheckEnabled(env = process.env) {
  return !falseLike(env.CODELOCAL_UPDATE_CHECK);
}

function autoUpdateEnabled(env = process.env) {
  return !falseLike(env.CODELOCAL_AUTO_UPDATE);
}

function shouldAutoUpdate(args) {
  return Array.isArray(args) && args.length === 0;
}

function stateDir(env = process.env) {
  const configured = String(env.CODELOCAL_STATE_DIR || '').trim();
  return configured || path.join(os.homedir(), '.codelocal');
}

function cachePath(env = process.env) {
  return path.join(stateDir(env), 'update-check.json');
}

function readCache(env = process.env) {
  try {
    const parsed = JSON.parse(fs.readFileSync(cachePath(env), 'utf8'));
    if (!parseVersion(parsed.latestVersion) || !Number.isFinite(parsed.checkedAt)) return null;
    return parsed;
  } catch {
    return null;
  }
}

function writeCache(channel, latestVersion, env = process.env, now = Date.now()) {
  try {
    const dir = stateDir(env);
    fs.mkdirSync(dir, { recursive: true, mode: 0o700 });
    if (process.platform !== 'win32') fs.chmodSync(dir, 0o700);
    const destination = cachePath(env);
    const temporary = `${destination}.${process.pid}.tmp`;
    fs.writeFileSync(temporary, `${JSON.stringify({ checkedAt: now, channel, latestVersion }, null, 2)}\n`, { mode: 0o600 });
    fs.renameSync(temporary, destination);
    if (process.platform !== 'win32') fs.chmodSync(destination, 0o600);
  } catch {
    // Update caching is an optimization and must never block CodeLocal startup.
  }
}

function registryURL(env = process.env) {
  const configured = String(env.CODELOCAL_UPDATE_REGISTRY || env.npm_config_registry || DEFAULT_REGISTRY).trim();
  try {
    const url = new URL(configured);
    if (url.protocol !== 'https:' && url.protocol !== 'http:') return DEFAULT_REGISTRY;
    return url.toString().replace(/\/$/, '');
  } catch {
    return DEFAULT_REGISTRY;
  }
}

async function fetchLatest(channel, env = process.env, fetchImpl = globalThis.fetch) {
  if (typeof fetchImpl !== 'function') throw new Error('fetch is unavailable');
  const base = new URL(registryURL(env));
  base.pathname = `${base.pathname.replace(/\/$/, '')}/-/package/codelocal/dist-tags`;
  base.search = '';
  base.hash = '';
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), UPDATE_TIMEOUT_MS);
  try {
    const response = await fetchImpl(base, {
      headers: { Accept: 'application/json', 'User-Agent': 'codelocal-auto-update/1' },
      signal: controller.signal,
    });
    if (!response.ok) throw new Error(`registry returned ${response.status}`);
    const tags = await response.json();
    const latest = normalizeVersion(tags[channel]);
    if (!parseVersion(latest)) throw new Error('registry returned an invalid version');
    return latest;
  } finally {
    clearTimeout(timer);
  }
}

async function resolveLatest(installedVersion, channel, env = process.env, options = {}) {
  const now = options.now ? options.now() : Date.now();
  const cached = (options.readCache || readCache)(env);
  if (cached && cached.channel === channel && now >= cached.checkedAt && now - cached.checkedAt < UPDATE_CACHE_TTL_MS) {
    return cached.latestVersion;
  }
  const latest = await fetchLatest(channel, env, options.fetchImpl || globalThis.fetch);
  (options.writeCache || writeCache)(channel, latest, env, now);
  return latest;
}

function readInstalledVersion(packageRoot) {
  const manifest = JSON.parse(fs.readFileSync(path.join(packageRoot, 'package.json'), 'utf8'));
  const version = normalizeVersion(manifest.version);
  if (!parseVersion(version)) throw new Error('installed package has an invalid version');
  return version;
}

function npmCommand() {
  return process.platform === 'win32' ? 'npm.cmd' : 'npm';
}

function npmInvocation(version, env = process.env) {
  const args = ['install', '-g', `codelocal@${version}`, '--no-audit', '--no-fund', '--registry', registryURL(env)];
  return { command: npmCommand(), args };
}

function comparablePath(value) {
  try {
    return fs.realpathSync(value);
  } catch {
    return path.resolve(value);
  }
}

function isGlobalInstall(packageRoot, env = process.env, run = spawnSync) {
  const result = run(npmCommand(), ['root', '-g', '--silent'], { encoding: 'utf8', env });
  if (result.error || result.status !== 0 || !String(result.stdout || '').trim()) return false;
  const expected = path.join(String(result.stdout).trim(), 'codelocal');
  return comparablePath(expected) === comparablePath(packageRoot);
}

function runtimePID(env = process.env) {
  try {
    const payload = JSON.parse(fs.readFileSync(path.join(stateDir(env), 'runtime.lock'), 'utf8'));
    return Number.isInteger(payload.pid) && payload.pid > 0 ? payload.pid : 0;
  } catch {
    return 0;
  }
}

function processAlive(pid) {
  if (!pid) return false;
  try {
    process.kill(pid, 0);
    return true;
  } catch (error) {
    return error && error.code === 'EPERM';
  }
}

function delay(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

async function stopRuntimeBeforeUpdate(packageRoot, env = process.env, run = spawnSync, options = {}) {
  const pid = (options.runtimePID || runtimePID)(env);
  if (!pid) return true;
  const rc = runNative(packageRoot, ['stop'], env, { spawnSync: run, stdio: 'ignore' });
  if (rc !== 0) return false;
  const alive = options.processAlive || processAlive;
  const sleep = options.delay || delay;
  for (let attempt = 0; attempt < 50; attempt += 1) {
    if (!alive(pid)) return true;
    await sleep(100);
  }
  return !alive(pid);
}

async function maybeAutoUpdate(packageRoot, args, env = process.env, options = {}) {
  if (!updateCheckEnabled(env) || !autoUpdateEnabled(env) || env.CODELOCAL_AUTO_UPDATE_RESTART === '1' || !shouldAutoUpdate(args)) {
    return { restarted: false };
  }

  let installedVersion;
  let channel;
  let latestVersion;
  try {
    installedVersion = (options.readInstalledVersion || readInstalledVersion)(packageRoot);
    channel = releaseChannel(installedVersion, env);
    latestVersion = await resolveLatest(installedVersion, channel, env, options);
  } catch {
    return { restarted: false };
  }

  const comparison = compareVersions(installedVersion, latestVersion);
  if (comparison === null || comparison >= 0) return { restarted: false };

  const run = options.spawnSync || spawnSync;
  const globalInstall = options.isGlobalInstall
    ? options.isGlobalInstall(packageRoot, env, run)
    : isGlobalInstall(packageRoot, env, run);
  if (!globalInstall) return { restarted: false };

  const log = options.log || ((message) => console.error(message));
  const runtimeStopped = options.stopRuntimeBeforeUpdate
    ? await options.stopRuntimeBeforeUpdate(packageRoot, env, run)
    : await stopRuntimeBeforeUpdate(packageRoot, env, run, options);
  if (!runtimeStopped) {
    log('CodeLocal found a newer package, but the current runtime did not stop cleanly. Update deferred to keep the active runtime safe.');
    return { restarted: false, failed: true };
  }

  log(`CodeLocal ${installedVersion} → ${latestVersion}. Updating automatically...`);
  const invocation = npmInvocation(latestVersion, env);
  const install = run(invocation.command, invocation.args, { stdio: 'inherit', env });
  if (install.error || install.status !== 0) {
    const reason = install.error ? install.error.message : `npm exited with status ${install.status}`;
    log(`CodeLocal auto-update failed (${reason}). Continuing with ${installedVersion}.`);
    log(`Manual update: npm install -g codelocal@${channel}`);
    return { restarted: false, failed: true };
  }

  let updatedVersion;
  try {
    updatedVersion = (options.readInstalledVersion || readInstalledVersion)(packageRoot);
  } catch (error) {
    log(`CodeLocal auto-update could not verify the installed version (${error.message}). Continuing with ${installedVersion}.`);
    return { restarted: false, failed: true };
  }
  if (compareVersions(updatedVersion, latestVersion) !== 0) {
    log(`CodeLocal auto-update expected ${latestVersion} but found ${updatedVersion}. Continuing without restart.`);
    return { restarted: false, failed: true };
  }

  log(`CodeLocal updated to ${updatedVersion}. Restarting...`);
  const script = options.scriptPath || __filename;
  const restart = run(process.execPath, [script, ...args], {
    stdio: 'inherit',
    env: { ...env, CODELOCAL_AUTO_UPDATE_RESTART: '1' },
  });
  if (restart.error) {
    log(`CodeLocal updated, but automatic restart failed: ${restart.error.message}`);
    return { restarted: false, failed: true };
  }
  return { restarted: true, status: restart.status, signal: restart.signal };
}

function platformFiles() {
  return {
    files: {
      'darwin-arm64': 'codelocal-darwin-arm64',
      'darwin-x64': 'codelocal-darwin-x64',
      'linux-arm64': 'codelocal-linux-arm64',
      'linux-x64': 'codelocal-linux-x64',
      'win32-x64': 'codelocal-win32-x64.exe',
      'win32-arm64': 'codelocal-win32-arm64.exe',
    },
    helperFiles: {
      'darwin-arm64': 'computer-darwin-arm64',
      'darwin-x64': 'computer-darwin-amd64',
      'linux-arm64': 'computer-linux-arm64',
      'linux-x64': 'computer-linux-amd64',
      'win32-x64': 'computer-windows-amd64.exe',
      'win32-arm64': 'computer-windows-arm64.exe',
    },
  };
}

function runNative(packageRoot, args, env = process.env, options = {}) {
  const key = `${process.platform}-${process.arch}`;
  const { files, helperFiles } = platformFiles();
  const file = files[key];
  const helperFile = helperFiles[key];
  if (!file || !helperFile) {
    console.error(`CodeLocal does not have native binaries for ${key}.`);
    return 1;
  }
  const playwrightCli = path.join(packageRoot, 'node_modules', '.bin', process.platform === 'win32' ? 'playwright-cli.cmd' : 'playwright-cli');
  const binary = path.join(__dirname, 'native', file);
  const computerHelper = path.join(__dirname, 'helpers', helperFile);
  const childEnv = {
    ...env,
    CODELOCAL_PACKAGE_ROOT: packageRoot,
    CODELOCAL_PLAYWRIGHT_CLI: env.CODELOCAL_PLAYWRIGHT_CLI || playwrightCli,
    CODELOCAL_COMPUTER_HELPER: env.CODELOCAL_COMPUTER_HELPER || computerHelper,
  };
  const result = (options.spawnSync || spawnSync)(binary, args, { stdio: options.stdio || 'inherit', env: childEnv });
  if (result.error) {
    console.error(result.error.message);
    return 1;
  }
  if (result.signal) {
    process.kill(process.pid, result.signal);
    return 1;
  }
  return result.status ?? 1;
}

async function main() {
  const packageRoot = path.resolve(__dirname, '..');
  const args = process.argv.slice(2);
  const update = await maybeAutoUpdate(packageRoot, args, process.env);
  if (update.restarted) {
    if (update.signal) process.kill(process.pid, update.signal);
    process.exit(update.status ?? 1);
  }
  process.exit(runNative(packageRoot, args, process.env));
}

if (require.main === module) {
  main().catch((error) => {
    console.error(error && error.message ? error.message : String(error));
    process.exit(1);
  });
}

module.exports = {
  autoUpdateEnabled,
  compareVersions,
  fetchLatest,
  maybeAutoUpdate,
  normalizeVersion,
  npmInvocation,
  readCache,
  releaseChannel,
  resolveLatest,
  shouldAutoUpdate,
  stopRuntimeBeforeUpdate,
  updateCheckEnabled,
  writeCache,
};
