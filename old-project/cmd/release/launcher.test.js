const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const test = require('node:test');
const launcher = require('./launcher.js');

test('compareVersions handles stable and prerelease semver', () => {
  assert.equal(launcher.compareVersions('1.5.70', '1.5.71'), -1);
  assert.equal(launcher.compareVersions('1.5.71', '1.5.71'), 0);
  assert.equal(launcher.compareVersions('1.6.0-beta.2', '1.6.0-beta.10'), -1);
  assert.equal(launcher.compareVersions('1.6.0-beta.10', '1.6.0'), -1);
  assert.equal(launcher.compareVersions('2.0.0', '1.9.9'), 1);
});

test('releaseChannel infers stable/beta and honors explicit channel', () => {
  assert.equal(launcher.releaseChannel('1.5.71', {}), 'latest');
  assert.equal(launcher.releaseChannel('1.6.0-beta.1', {}), 'beta');
  assert.equal(launcher.releaseChannel('1.5.71', { CODELOCAL_RELEASE_CHANNEL: 'canary' }), 'canary');
});

test('auto update runs only for normal runtime startup and respects opt-out', () => {
  assert.equal(launcher.autoUpdateEnabled({}), true);
  assert.equal(launcher.autoUpdateEnabled({ CODELOCAL_AUTO_UPDATE: '0' }), false);
  assert.equal(launcher.updateCheckEnabled({ CODELOCAL_UPDATE_CHECK: 'false' }), false);
  assert.equal(launcher.shouldAutoUpdate([]), true);
  assert.equal(launcher.shouldAutoUpdate(['status']), false);
  assert.equal(launcher.shouldAutoUpdate(['uninstall', '--all']), false);
  assert.equal(launcher.shouldAutoUpdate(['.']), false);
});

test('resolveLatest reuses the shared fresh update cache without network', async () => {
  let fetches = 0;
  const latest = await launcher.resolveLatest('1.5.70', 'latest', {}, {
    now: () => 10_000,
    readCache: () => ({ checkedAt: 9_500, channel: 'latest', latestVersion: '1.5.71' }),
    fetchImpl: async () => {
      fetches += 1;
      throw new Error('network should not be used');
    },
  });
  assert.equal(latest, '1.5.71');
  assert.equal(fetches, 0);
});

test('writeCache is compatible with the Go startup cache schema', () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'codelocal-launcher-'));
  const env = { CODELOCAL_STATE_DIR: dir };
  launcher.writeCache('latest', '1.5.71', env, 1234);
  const payload = JSON.parse(fs.readFileSync(path.join(dir, 'update-check.json'), 'utf8'));
  assert.deepEqual(payload, { checkedAt: 1234, channel: 'latest', latestVersion: '1.5.71' });
});

test('stopRuntimeBeforeUpdate waits for the active runtime to exit', async () => {
  let aliveChecks = 0;
  let invocation = null;
  const stopped = await launcher.stopRuntimeBeforeUpdate('/package', {}, (command, args, options) => {
    invocation = { command, args, stdio: options.stdio };
    return { status: 0, signal: null };
  }, {
    runtimePID: () => 12345,
    processAlive: () => {
      aliveChecks += 1;
      return aliveChecks < 2;
    },
    delay: async () => {},
  });
  assert.equal(stopped, true);
  assert.deepEqual(invocation.args, ['stop']);
  assert.equal(invocation.stdio, 'ignore');
});

test('maybeAutoUpdate updates and restarts only after exact version verification', async () => {
  const calls = [];
  let reads = 0;
  const logs = [];
  const result = await launcher.maybeAutoUpdate('/package', [], {}, {
    readInstalledVersion: () => (++reads === 1 ? '1.5.70' : '1.5.71'),
    readCache: () => ({ checkedAt: Date.now(), channel: 'latest', latestVersion: '1.5.71' }),
    isGlobalInstall: () => true,
    stopRuntimeBeforeUpdate: async () => true,
    spawnSync: (command, args, options) => {
      calls.push({ command, args, env: options.env });
      return { status: 0, signal: null };
    },
    scriptPath: '/package/bin/codelocal.js',
    log: (message) => logs.push(message),
  });
  assert.equal(result.restarted, true);
  assert.equal(calls.length, 2);
  assert.match(calls[0].args.join(' '), /install -g codelocal@1\.5\.71/);
  assert.equal(calls[1].command, process.execPath);
  assert.equal(calls[1].env.CODELOCAL_AUTO_UPDATE_RESTART, '1');
  assert.ok(logs.some((line) => line.includes('Restarting')));
});

test('maybeAutoUpdate does not mutate local or staging installs', async () => {
  let spawned = false;
  const result = await launcher.maybeAutoUpdate('/package', [], {}, {
    readInstalledVersion: () => '1.5.70',
    readCache: () => ({ checkedAt: Date.now(), channel: 'latest', latestVersion: '1.5.71' }),
    isGlobalInstall: () => false,
    spawnSync: () => {
      spawned = true;
      return { status: 0 };
    },
  });
  assert.equal(result.restarted, false);
  assert.equal(spawned, false);
});

test('maybeAutoUpdate falls back to current runtime when npm update fails', async () => {
  const logs = [];
  const result = await launcher.maybeAutoUpdate('/package', [], {}, {
    readInstalledVersion: () => '1.5.70',
    readCache: () => ({ checkedAt: Date.now(), channel: 'latest', latestVersion: '1.5.71' }),
    isGlobalInstall: () => true,
    stopRuntimeBeforeUpdate: async () => true,
    spawnSync: () => ({ status: 1, signal: null }),
    log: (message) => logs.push(message),
  });
  assert.equal(result.restarted, false);
  assert.equal(result.failed, true);
  assert.ok(logs.some((line) => line.includes('Continuing with 1.5.70')));
});
