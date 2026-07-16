import { access, readFile } from "node:fs/promises";
import { join } from "node:path";
import { dirname } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const dist = join(root, "dist");
const html = await readFile(join(dist, "index.html"), "utf8");

const checks = [
  ["primary headline", "Put AI coding agents"],
  ["early-access section", "early-access"],
  ["GitHub destination", "github.com/mtdewwolf/Retcon"],
  ["privacy disclosure", "Tally processes this form"],
  ["social metadata", "og-retcon.png"],
];

for (const [name, value] of checks) {
  if (!html.includes(value)) throw new Error(`Built site is missing ${name}.`);
}

await Promise.all([
  access(join(dist, "assets", "brand", "favicon.svg")),
  access(join(dist, "assets", "brand", "retcon-icon-192.png")),
  access(join(dist, "assets", "brand", "retcon-icon-512.png")),
  access(join(dist, "assets", "social", "og-retcon.png")),
  access(join(dist, "site.webmanifest")),
  access(join(dist, "robots.txt")),
]);

console.log("Built website verified.");
