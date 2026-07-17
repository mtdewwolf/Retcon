import { existsSync } from "node:fs";
import { chromium } from "playwright";

const executablePath = process.env.RETCON_CHROMIUM_PATH ?? chromium.executablePath();
if (!existsSync(executablePath)) {
  throw new Error(`managed Chromium is missing at ${executablePath}; run bun run browser:install`);
}
console.log(JSON.stringify({ status: "launching", executablePath }));
const browser = await chromium.launch({
  headless: true,
  executablePath,
});
console.log(JSON.stringify({ status: "launched", version: browser.version() }));
const page = await browser.newPage();
await page.goto("data:text/html,<title>Retcon proof</title>");
console.log(JSON.stringify({ title: await page.title() }));
await browser.close();
console.log(JSON.stringify({ status: "closed" }));
