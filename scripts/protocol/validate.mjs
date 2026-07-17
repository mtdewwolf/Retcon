#!/usr/bin/env node
/**
 * Validate schemas/protocol/v1.json and tests/fixtures/protocol/*.json
 */
import { readFileSync, readdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "../..");
const schemaPath = join(root, "schemas/protocol/v1.json");
const fixturesDir = join(root, "tests/fixtures/protocol");

const schema = JSON.parse(readFileSync(schemaPath, "utf8"));

if (!schema.methodCatalog) {
  throw new Error("v1.json is missing methodCatalog");
}

const namespaces = [
  "core",
  "events",
  "jobs",
  "project",
  "provider",
  "session",
  "turn",
  "terminal",
  "git",
  "agent",
  "browser",
];
for (const namespace of namespaces) {
  const methods = schema.methodCatalog[namespace];
  if (!Array.isArray(methods) || methods.length === 0) {
    throw new Error(`methodCatalog.${namespace} must be a non-empty array`);
  }
  for (const method of methods) {
    if (!method.startsWith(`${namespace}.`)) {
      throw new Error(`method ${method} is not in namespace ${namespace}`);
    }
  }
}

const allMethods = new Set(
  Object.values(schema.methodCatalog).flatMap((methods) => methods),
);
if (allMethods.size !== Object.values(schema.methodCatalog).flat().length) {
  throw new Error("methodCatalog contains duplicate method names");
}

for (const file of readdirSync(fixturesDir)) {
  if (!file.endsWith(".json")) continue;
  const text = readFileSync(join(fixturesDir, file), "utf8").trim();
  JSON.parse(text);
  const valid = file.startsWith("valid-");
  const invalid = file.startsWith("invalid-");
  if (!valid && !invalid) {
    throw new Error(`fixture ${file} must start with valid- or invalid-`);
  }
  const value = JSON.parse(text);
  if (invalid) {
    const extraField =
      value.id !== undefined &&
      value.method !== undefined &&
      Object.keys(value).length > 3;
    const shortAuth =
      value.auth !== undefined && String(value.auth).length < 32;
    if (!extraField && !shortAuth) {
      throw new Error(`invalid fixture ${file} does not look invalid`);
    }
  }
}

console.log(
  `protocol schema OK (${allMethods.size} methods, ${readdirSync(fixturesDir).filter((f) => f.endsWith(".json")).length} fixtures)`,
);
