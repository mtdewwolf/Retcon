#!/usr/bin/env node

// Verifies repository and package license metadata. Invoked by scripts/verify.mjs
// on developer machines and CI, and independently by the dependency-audit workflow.

import { readFileSync, readdirSync } from 'node:fs';
import { dirname, join, relative } from 'node:path';
import { fileURLToPath } from 'node:url';

const scriptDirectory = dirname(fileURLToPath(import.meta.url));
const repositoryRoot = join(scriptDirectory, '..');
const expectedSpdxLicense = 'Apache-2.0';
const rootLicensePath = join(repositoryRoot, 'LICENSE');
const normalizeLicense = (text) => `${text.replaceAll('\r\n', '\n').trimEnd()}\n`;
const rootLicense = normalizeLicense(readFileSync(rootLicensePath, 'utf8'));
const failures = [];

if (!rootLicense.includes('Apache License') || !rootLicense.includes('Version 2.0')) {
  failures.push('LICENSE is not the Apache License 2.0 text.');
}

const cargoManifest = readFileSync(join(repositoryRoot, 'Cargo.toml'), 'utf8');
if (!cargoManifest.match(/^license\s*=\s*['"]Apache-2\.0['"]\s*$/m)) {
  failures.push('Cargo.toml workspace.package.license must be Apache-2.0.');
}

const packageDirectory = join(repositoryRoot, 'packages');
const flutterPackages = readdirSync(packageDirectory, { withFileTypes: true })
  .filter((entry) => entry.isDirectory())
  .map((entry) => join(packageDirectory, entry.name))
  .filter((directory) => {
    try {
      readFileSync(join(directory, 'pubspec.yaml'));
      return true;
    } catch {
      return false;
    }
  });

for (const directory of flutterPackages) {
  const licensePath = join(directory, 'LICENSE');
  let packageLicense;
  try {
    packageLicense = normalizeLicense(readFileSync(licensePath, 'utf8'));
  } catch {
    failures.push(`${relative(repositoryRoot, directory)} is missing LICENSE.`);
    continue;
  }

  if (packageLicense !== rootLicense) {
    failures.push(`${relative(repositoryRoot, licensePath)} must match the root LICENSE.`);
  }
}

for (const application of readdirSync(join(repositoryRoot, 'apps'), { withFileTypes: true })) {
  if (!application.isDirectory()) continue;
  const manifestPath = join(repositoryRoot, 'apps', application.name, 'package.json');
  let manifest;
  try {
    manifest = JSON.parse(readFileSync(manifestPath, 'utf8'));
  } catch (error) {
    if (error?.code === 'ENOENT') continue;
    failures.push(`${relative(repositoryRoot, manifestPath)} is not valid JSON.`);
    continue;
  }

  if (manifest.private !== true && manifest.license !== expectedSpdxLicense) {
    failures.push(
      `${relative(repositoryRoot, manifestPath)} must declare license ${expectedSpdxLicense}.`,
    );
  }
  if (manifest.license && manifest.license !== expectedSpdxLicense) {
    failures.push(
      `${relative(repositoryRoot, manifestPath)} declares unexpected license ${manifest.license}.`,
    );
  }
}

if (failures.length > 0) {
  console.error(`License verification failed (${failures.length} issue(s)):`);
  for (const failure of failures) console.error(`  - ${failure}`);
  process.exit(1);
}

console.log(
  `License verification passed: ${flutterPackages.length} Flutter packages and repository manifests use ${expectedSpdxLicense}.`,
);
