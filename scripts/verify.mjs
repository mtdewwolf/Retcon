#!/usr/bin/env node

// Runs the canonical repository verification inventory on developer machines and CI.
// Successful command output is condensed; full output is printed for failed checks.

import { spawnSync } from 'node:child_process';
import { dirname, join, relative } from 'node:path';
import { fileURLToPath } from 'node:url';

const scriptDirectory = dirname(fileURLToPath(import.meta.url));
const repositoryRoot = join(scriptDirectory, '..');
const isWindows = process.platform === 'win32';

const quoteWindowsArgument = (value) => {
  if (/^[A-Za-z0-9_./:\\-]+$/.test(value)) return value;
  return `"${value.replaceAll('"', '""')}"`;
};

const flutterProjects = [
  ['flutter-desktop', 'Desktop shell', 'apps/desktop'],
  ['flutter-test-dashboard', 'Test dashboard', 'apps/test-dashboard'],
  ['flutter-design-system', 'Design system', 'packages/retcon-design-system'],
  ['flutter-diff-viewer', 'Diff viewer', 'packages/retcon-diff-viewer'],
  ['flutter-file-viewer', 'File viewer', 'packages/retcon-file-viewer'],
  ['flutter-terminal-view', 'Terminal view', 'packages/retcon-terminal-view'],
];

const command = (id, suite, label, executable, args, options = {}) => ({
  id,
  suite,
  label,
  executable,
  args,
  cwd: repositoryRoot,
  ...options,
});

const checks = [
  command('licenses', 'protocol', 'Repository license metadata', 'node', [
    'scripts/verify-licenses.mjs',
  ]),
  command('protocol-schema', 'protocol', 'Protocol schema and fixtures', 'node', [
    'scripts/protocol/validate.mjs',
  ]),
  command('protocol-typescript', 'protocol', 'Generate TypeScript protocol client', 'node', [
    'scripts/protocol/generate-typescript.mjs',
  ]),
  command('protocol-dart', 'protocol', 'Generate Dart protocol client', 'node', [
    'scripts/protocol/generate-dart.mjs',
  ]),
  command('protocol-drift', 'protocol', 'Generated protocol clients are current', 'git', [
    'diff',
    '--exit-code',
    '--',
    'apps/browser-service/src/generated',
    'apps/desktop/lib/src/generated',
  ]),
  command('protocol-tests', 'protocol', 'Protocol fixture tests', 'cargo', [
    'test',
    '-p',
    'retcon-protocol',
  ]),
  command('rust-format', 'rust', 'Rust formatting', 'cargo', ['fmt', '--all', '--check']),
  command('rust-clippy', 'rust', 'Rust Clippy', 'cargo', [
    'clippy',
    '--workspace',
    '--all-targets',
    '--',
    '-D',
    'warnings',
  ]),
  command('rust-tests', 'rust', 'Rust workspace tests', 'cargo', ['test', '--workspace']),
  ...flutterProjects.flatMap(([id, label, path]) => [
    command(`${id}-dependencies`, 'flutter', `${label}: dependencies`, 'flutter', ['pub', 'get'], {
      cwd: join(repositoryRoot, path),
      checkId: id,
    }),
    command(`${id}-analyze`, 'flutter', `${label}: analyze`, 'flutter', ['analyze'], {
      cwd: join(repositoryRoot, path),
      checkId: id,
    }),
    command(`${id}-tests`, 'flutter', `${label}: tests`, 'flutter', ['test'], {
      cwd: join(repositoryRoot, path),
      checkId: id,
    }),
  ]),
  command('browser-dependencies', 'browser', 'Browser service: dependencies', 'bun', [
    'install',
    '--frozen-lockfile',
  ], { cwd: join(repositoryRoot, 'apps/browser-service'), checkId: 'browser-service' }),
  command('browser-chromium', 'browser', 'Browser service: managed Chromium', 'bunx', [
    'playwright',
    'install',
    ...(process.platform === 'linux' ? ['--with-deps'] : []),
    'chromium',
  ], { cwd: join(repositoryRoot, 'apps/browser-service'), checkId: 'browser-service' }),
  command('browser-lint', 'browser', 'Browser service: lint source, tests, and scripts', 'bun', [
    'run',
    'lint',
  ], { cwd: join(repositoryRoot, 'apps/browser-service'), checkId: 'browser-service' }),
  command('browser-typecheck', 'browser', 'Browser service: typecheck', 'bun', [
    'run',
    'typecheck',
  ], { cwd: join(repositoryRoot, 'apps/browser-service'), checkId: 'browser-service' }),
  command('browser-tests', 'browser', 'Browser service: tests', 'bun', ['test'], {
    cwd: join(repositoryRoot, 'apps/browser-service'),
    checkId: 'browser-service',
    env: { RETCON_BROWSER_E2E: '1' },
  }),
  command('website-dependencies', 'website', 'Website: dependencies', 'npm', ['ci'], {
    cwd: join(repositoryRoot, 'apps/website'),
    checkId: 'website-build',
  }),
  command('website-build', 'website', 'Website: production build and verification', 'npm', [
    'test',
  ], { cwd: join(repositoryRoot, 'apps/website'), checkId: 'website-build' }),
  command('windows-rust-release', 'windows-build', 'Windows Rust release build', 'cargo', [
    'build',
    '--workspace',
    '--release',
  ], { platforms: ['win32'] }),
  command('windows-desktop-dependencies', 'windows-build', 'Windows desktop: dependencies', 'flutter', [
    'pub',
    'get',
  ], { cwd: join(repositoryRoot, 'apps/desktop'), platforms: ['win32'] }),
  command('windows-desktop-release', 'windows-build', 'Windows desktop release build', 'flutter', [
    'build',
    'windows',
    '--release',
  ], { cwd: join(repositoryRoot, 'apps/desktop'), platforms: ['win32'] }),
];

const manualGates = [
  {
    id: 'windows-shell-display',
    label: 'Win10/11 shell, mixed-DPI, multi-monitor, sleep/resume, keyboard, and screen reader',
    command: 'scripts/phase2/01-windows-shell.ps1 and 02-multi-monitor-sleep.ps1',
    stack: 'powershell',
  },
  {
    id: 'interactive-conpty',
    label: 'Interactive ConPTY ANSI, Unicode, resize, and reflow',
    command: 'scripts/phase2/03-terminals.ps1',
    stack: 'powershell',
  },
  {
    id: 'authenticated-provider',
    label: 'Authenticated provider turn, resume, and cancellation',
    command: 'scripts/phase2/04-agent-e2e.ps1',
    stack: 'powershell',
  },
];

const dashboardSuites = [
  {
    id: 'protocol',
    label: 'Protocol and package metadata',
    stack: 'protocol',
    path: '.',
    args: ['scripts/verify.mjs', '--suite', 'protocol'],
    ciJob: 'Protocol (schema + fixtures)',
    description: 'Schema, fixtures, generated-client drift, protocol tests, and license metadata.',
  },
  {
    id: 'rust-workspace',
    label: 'Rust workspace',
    stack: 'rust',
    path: '.',
    args: ['scripts/verify.mjs', '--suite', 'rust'],
    ciJob: 'Rust (fmt, clippy, test)',
    description: 'Formatting, Clippy, and all workspace tests.',
  },
  ...flutterProjects.map(([id, label, path]) => ({
    id,
    label,
    stack: 'flutter',
    path,
    args: ['scripts/verify.mjs', '--check', id],
    ciJob: 'Flutter (all apps/packages analyze + test)',
    description: 'Dependency resolution, static analysis, and Flutter tests.',
    testFileGlob: 'test/**/*_test.dart',
  })),
  {
    id: 'browser-service',
    label: 'Browser service',
    stack: 'bun',
    path: 'apps/browser-service',
    args: ['scripts/verify.mjs', '--suite', 'browser'],
    ciJob: 'Browser service (lint, typecheck, test)',
    description: 'Locked install, managed Chromium, lint, typecheck, and tests.',
    testFileGlob: 'test/**/*.test.ts',
  },
  {
    id: 'website-build',
    label: 'Website build verify',
    stack: 'node',
    path: 'apps/website',
    args: ['scripts/verify.mjs', '--suite', 'website'],
    ciJob: 'Website (build and verify)',
    description: 'Locked install, Astro production build, and asset verification.',
  },
  {
    id: 'windows-build',
    label: 'Windows release build',
    stack: 'flutter',
    path: '.',
    args: ['scripts/verify.mjs', '--suite', 'windows-build'],
    ciJob: 'Windows build',
    description: 'Release builds for the Rust workspace and Flutter desktop shell.',
  },
];

const args = process.argv.slice(2);
const requestedSuites = [];
const requestedChecks = [];
let listOnly = false;
let json = false;

for (let index = 0; index < args.length; index += 1) {
  const argument = args[index];
  if (argument === '--suite') requestedSuites.push(args[++index]);
  else if (argument === '--check') requestedChecks.push(args[++index]);
  else if (argument === '--list') listOnly = true;
  else if (argument === '--json') json = true;
  else if (argument === '--help' || argument === '-h') {
    console.log('Usage: node scripts/verify.mjs [--suite NAME] [--check ID] [--list [--json]]');
    process.exit(0);
  } else {
    console.error(`Unknown argument: ${argument}`);
    process.exit(2);
  }
}

const suiteNames = [...new Set(checks.map((item) => item.suite))];
const checkNames = [...new Set(checks.map((item) => item.checkId ?? item.id))];
for (const suite of requestedSuites) {
  if (!suiteNames.includes(suite)) {
    console.error(`Unknown suite: ${suite}. Expected one of: ${suiteNames.join(', ')}`);
    process.exit(2);
  }
}
for (const checkId of requestedChecks) {
  if (!checkNames.includes(checkId)) {
    console.error(`Unknown check: ${checkId}. Run --list to inspect the inventory.`);
    process.exit(2);
  }
}

const inventory = {
  suites: suiteNames.map((suite) => ({
    id: suite,
    checks: checks
      .filter((item) => item.suite === suite)
      .map((item) => item.checkId ?? item.id)
      .filter((value, index, values) => values.indexOf(value) === index),
  })),
  flutterProjects: flutterProjects.map(([id, label, path]) => ({ id, label, path })),
  dashboardSuites,
  manualGates,
};

if (listOnly) {
  if (json) console.log(JSON.stringify(inventory, null, 2));
  else {
    console.log('Required verification suites:');
    for (const suite of inventory.suites) console.log(`  ${suite.id}: ${suite.checks.join(', ')}`);
    console.log('\nManual/release gates:');
    for (const gate of manualGates) console.log(`  ${gate.id}: ${gate.label} (${gate.command})`);
  }
  process.exit(0);
}

let selected = checks;
if (requestedSuites.length > 0) {
  selected = selected.filter((item) => requestedSuites.includes(item.suite));
}
if (requestedChecks.length > 0) {
  selected = selected.filter((item) => requestedChecks.includes(item.checkId ?? item.id));
}

const results = [];
console.log(`Retcon verification: ${selected.length} command(s)`);

for (const item of selected) {
  if (item.platforms && !item.platforms.includes(process.platform)) {
    results.push({ item, status: 'SKIP', duration: 0 });
    console.log(`SKIP ${item.label} (requires ${item.platforms.join(' or ')})`);
    continue;
  }

  const started = Date.now();
  const spawnOptions = {
    cwd: item.cwd,
    env: {
      ...process.env,
      PLAYWRIGHT_SKIP_BROWSER_DOWNLOAD: '1',
      ...item.env,
    },
    encoding: 'utf8',
    maxBuffer: 64 * 1024 * 1024,
  };
  const result = isWindows
    ? spawnSync(
        [item.executable, ...item.args].map(quoteWindowsArgument).join(' '),
        { ...spawnOptions, shell: true },
      )
    : spawnSync(item.executable, item.args, spawnOptions);
  const duration = Date.now() - started;
  const status = result.status === 0 ? 'PASS' : 'FAIL';
  results.push({ item, status, duration });
  console.log(`${status} ${item.label} (${(duration / 1000).toFixed(1)}s)`);

  if (status === 'FAIL') {
    console.error(`\n--- ${item.label} output (${relative(repositoryRoot, item.cwd) || '.'}) ---`);
    if (result.stdout) process.stderr.write(result.stdout);
    if (result.stderr) process.stderr.write(result.stderr);
    if (result.error) console.error(result.error.message);
    console.error('--- end output ---\n');
  }
}

const passed = results.filter((result) => result.status === 'PASS').length;
const failed = results.filter((result) => result.status === 'FAIL').length;
const skipped = results.filter((result) => result.status === 'SKIP').length;
console.log(`\nSummary: ${passed} passed, ${failed} failed, ${skipped} skipped.`);
if (failed > 0) {
  console.log('Failed checks:');
  for (const result of results.filter((entry) => entry.status === 'FAIL')) {
    console.log(`  - ${result.item.id}: ${result.item.label}`);
  }
  process.exit(1);
}
