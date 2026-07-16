import { chromium } from "playwright";

console.log("launching");
const executablePath = process.env.RETCON_CHROMIUM_PATH;
const browser = await chromium.launch({
  headless: true,
  ...(executablePath ? { executablePath } : {}),
});
console.log("launched");
const page = await browser.newPage();
await page.goto("data:text/html,<title>Retcon proof</title>");
console.log(await page.title());
await browser.close();
console.log("closed");
