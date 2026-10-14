// Takes one screenshot of one address of the running site.
//
//   node scripts/one-shot.mjs <address> <out.png> [light|dark] [scroll-px] [width] [height]

import puppeteer from "puppeteer-core";

const [address, out, theme = "light", scroll = "0", width = "1440", height = "900"] = process.argv.slice(2);
const chrome = process.env.CHROME ?? "C:/Program Files/Google/Chrome/Application/chrome.exe";
const browser = await puppeteer.launch({
  executablePath: chrome,
  headless: true,
  args: [`--window-size=${width},${height}`, "--use-angle=swiftshader", "--enable-unsafe-swiftshader", "--ignore-gpu-blocklist", "--hide-scrollbars"],
  defaultViewport: { width: Number(width), height: Number(height), deviceScaleFactor: 1 },
});
const page = await browser.newPage();
const problems = [];
page.on("console", (m) => (m.type() === "error" || m.type() === "warning") && problems.push(`${m.type()}: ${m.text()}`));
page.on("pageerror", (e) => problems.push(`uncaught: ${e.message}`));
await page.evaluateOnNewDocument((t) => localStorage.setItem("syndicate-theme", t), theme);
await page.goto(address, { waitUntil: "networkidle0", timeout: 120000 });
await new Promise((r) => setTimeout(r, 2500));
await page.evaluate((y) => document.querySelector(".view--scroll")?.scrollTo(0, y), Number(scroll));
await new Promise((r) => setTimeout(r, 1500));
await page.screenshot({ path: out, optimizeForSpeed: true });
await browser.close();
console.log(out, problems.length ? problems.join("\n") : "no page errors");
