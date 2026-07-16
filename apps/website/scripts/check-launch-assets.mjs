import sharp from "sharp";
import { existsSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const required = ["supervision.webp", "approval-review.webp", "browser-verification.webp"];
const missing = required.filter((file) => !existsSync(join(root, "public", "assets", "product", file)));

if (missing.length) {
  console.error(`Launch blocked: missing verified product captures: ${missing.join(", ")}`);
  process.exit(1);
}

for (const file of required) {
  const metadata = await sharp(join(root, "public", "assets", "product", file)).metadata();
  if (!metadata.width || !metadata.height || metadata.width < 1200 || metadata.height < 750) {
    console.error(`Launch blocked: ${file} must be at least 1200×750 pixels.`);
    process.exit(1);
  }
}

const tallyId = process.env.PUBLIC_TALLY_FORM_ID?.trim();
if (!tallyId || !/^[A-Za-z0-9]+$/.test(tallyId)) {
  console.error("Launch blocked: PUBLIC_TALLY_FORM_ID must contain the live Tally form ID.");
  process.exit(1);
}

console.log("Launch assets verified.");
