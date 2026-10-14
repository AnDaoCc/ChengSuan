import { Builder, By, until } from "selenium-webdriver";
import { spawn } from "node:child_process";
import fs from "node:fs/promises";
import path from "node:path";
import assert from "node:assert/strict";
import { DatabaseSync } from "node:sqlite";
const root = process.cwd();
const folder = path.join(
  root,
  "verification",
  "investment-1.1.0",
  `session-${Date.now()}`,
);
await fs.mkdir(folder, { recursive: true });
const executable = path.join(folder, "ChengSuan.exe");
await fs.copyFile(path.join(root, "release", "ChengSuan.exe"), executable);
const upgrade = process.argv.includes("--upgrade");
let oldPlan;
if (upgrade) {
  const data = path.join(folder, "ChengSuanData");
  await fs.mkdir(data);
  const db = new DatabaseSync(path.join(data, "chengsuan.db"));
  db.exec(
    "CREATE TABLE plans(id TEXT PRIMARY KEY,payload TEXT NOT NULL,updated_at TEXT NOT NULL);CREATE TABLE settings(id INTEGER PRIMARY KEY,payload TEXT NOT NULL);CREATE INDEX plans_updated ON plans(updated_at);PRAGMA user_version=2;PRAGMA application_id=1129532977;",
  );
  oldPlan = {
    schemaVersion: 1,
    id: "9837dd97-4990-47d3-9e96-3579bebf2430",
    name: "旧版升级验证方案",
    description: "隔离测试",
    currency: "CNY",
    items: [],
    rates: [],
    createdAt: "2026-10-12T00:00:00Z",
    updatedAt: "2026-10-12T00:00:00Z",
  };
  db.prepare("INSERT INTO plans VALUES(?,?,?)").run(
    oldPlan.id,
    JSON.stringify(oldPlan),
    oldPlan.updatedAt,
  );
  db.close();
}
const log = await fs.open(path.join(folder, "driver.log"), "w");
const processDriver = spawn(
  path.join(root, ".toolchains/cargo/bin/tauri-driver.exe"),
  [
    "--native-driver",
    path.join(root, ".toolchains/webdriver/edge154/msedgedriver.exe"),
    "--port",
    "4445",
    "--native-port",
    "9516",
  ],
  { windowsHide: true, stdio: ["ignore", log.fd, log.fd] },
);
let driver;
const report = {
  folder,
  startedAt: new Date().toISOString(),
  checks: [],
  issues: [],
};
const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
async function click(text) {
  await driver
    .wait(
      until.elementLocated(By.xpath(`//button[normalize-space(.)='${text}']`)),
      10000,
    )
    .click();
}
async function invoke(command, args = {}) {
  const value = await driver.executeAsyncScript(
    function (command, args, done) {
      window.__TAURI_INTERNALS__
        .invoke(command, args)
        .then((value) => done({ value }))
        .catch((error) => done({ error: String(error) }));
    },
    command,
    args,
  );
  if (value.error) throw Error(value.error);
  return value.value;
}
async function shot(name) {
  await fs.writeFile(
    path.join(folder, name + ".png"),
    Buffer.from(await driver.takeScreenshot(), "base64"),
  );
}
async function session(exe) {
  const result = await new Builder()
    .usingServer("http://127.0.0.1:4445")
    .withCapabilities({
      "tauri:options": { application: exe },
      browserName: "wry",
    })
    .build();
  await result.manage().setTimeouts({ script: 120000, implicit: 500 });
  return result;
}
try {
  for (let i = 0; i < 100; i++) {
    try {
      if ((await fetch("http://127.0.0.1:4445/status")).ok) break;
    } catch {}
    await sleep(100);
  }
  driver = await session(executable);
  await driver.wait(until.elementLocated(By.css(".sidebar")), 20000);
  report.devicePixelRatio = await driver.executeScript(
    "return window.devicePixelRatio",
  );
  const empty = await invoke("investment_load");
  assert.equal(empty.document.accounts.length, 1);
  assert.equal(empty.document.trades.length, 0);
  const costState = await invoke("load_state");
  assert.equal(costState.plans.length, upgrade ? 1 : 0);
  if (upgrade) {
    assert.deepEqual(costState.plans[0], oldPlan);
    assert.ok(
      (await fs.readdir(path.join(folder, "ChengSuanData", "backups"))).some(
        (n) => n.includes("before-migration"),
      ),
    );
    report.checks.push(
      "Actual EXE upgrades version 2 database with consistent backup and exact cost plan preservation",
    );
  }
  report.checks.push(
    "EXE alone creates portable data, empty cost plans and default investment account",
  );
  await driver.findElement(By.css('a[href="/investment"]')).click();
  await driver.wait(until.elementLocated(By.css(".investment-page")), 10000);
  await click("加密货币");
  await driver.wait(
    async () =>
      await driver
        .findElements(By.css(".market-table tbody tr"))
        .then((rows) => rows.length === 7),
    15000,
  );
  await shot("market-light");
  report.checks.push(
    "Seven default currencies shown in desktop market with Chinese names",
  );
  await driver
    .findElement(
      By.xpath(
        '//button[contains(@class,"asset-link")][.//strong[text()="比特币"]]',
      ),
    )
    .click();
  await driver.wait(until.elementLocated(By.css(".asset-detail")), 10000);
  await driver.wait(
    async () =>
      await driver
        .findElements(By.css(".chart-container canvas"))
        .then((rows) => rows.length > 0),
    40000,
  );
  await shot("bitcoin-chart-light");
  report.checks.push(
    "Real Bitcoin OHLC drawn with locally bundled candlesticks and MA lines",
  );
  await click("记录交易");
  const labels = await driver.findElements(By.css(".modal .field"));
  async function fill(label, value) {
    const field = await driver.findElement(
      By.xpath(
        `//div[contains(@class,'modal')]//label[span[text()='${label}']]//input`,
      ),
    );
    await field.clear();
    await field.sendKeys(value);
  }
  await fill("成交数量（币）", "0.01");
  await fill("实际成交单价", "80000");
  await fill("手续费（USDT）", "1");
  await fill("1 USDT = 多少人民币", "6.7");
  await click("保存交易");
  await driver.wait(
    async () => !(await driver.findElements(By.css(".modal"))).length,
    10000,
  );
  await click("我的持仓");
  await driver.wait(until.elementLocated(By.css(".holdings-table")), 10000);
  let saved = await invoke("investment_load");
  assert.equal(saved.document.trades.length, 1);
  assert.equal(saved.document.trades[0].quantity, "0.01");
  assert.equal(saved.document.trades[0].fxToCny, "6.7");
  await driver.wait(async () => {
    const s = await invoke("market_snapshot");
    return s.quotes.some((q) => q.instrumentId === "cg:bitcoin" && q.cnyRate);
  }, 45000);
  await sleep(300);
  await shot("portfolio-light");
  report.checks.push(
    "Desktop form buy + fee + frozen USDT FX is saved and appears in holdings",
  );
  await driver
    .findElement(By.css(".holdings-table .row-actions button:nth-child(2)"))
    .click();
  await fill("成交数量（币）", "0.1");
  await fill("实际成交单价", "90000");
  await fill("1 USDT = 多少人民币", "6.7");
  await click("保存交易");
  await driver.wait(until.elementLocated(By.css(".modal .warning")), 5000);
  assert.equal((await invoke("investment_load")).document.trades.length, 1);
  await driver.executeScript("window.confirm=()=>true");
  await click("取消");
  report.checks.push(
    "Oversell blocked in UI and native persistence; original transaction retained",
  );
  await driver
    .findElement(By.css(".holdings-table .row-actions button:nth-child(2)"))
    .click();
  await fill("成交数量（币）", "0.005");
  await fill("实际成交单价", "90000");
  await fill("手续费（USDT）", "1");
  await fill("1 USDT = 多少人民币", "6.7");
  await click("保存交易");
  await driver.wait(
    async () => !(await driver.findElements(By.css(".modal"))).length,
    10000,
  );
  saved = await invoke("investment_load");
  assert.equal(saved.document.trades.length, 2);
  const figures = await driver
    .findElement(By.css(".portfolio-summary"))
    .getText();
  assert.ok(
    figures.includes("2,683.35") && figures.includes("324.95"),
    figures,
  );
  await shot("portfolio-partial-sale");
  report.checks.push(
    "Partial sell updates remaining cost and realized gain with actual fees in the desktop UI",
  );
  const cfg = (await invoke("load_state")).settings;
  await invoke("save_settings", { settings: { ...cfg, theme: "dark" } });
  await driver.findElement(By.css('a[href="/settings"]')).click();
  await click("深色");
  await driver.findElement(By.css('a[href="/investment"]')).click();
  await click("我的持仓");
  await shot("portfolio-dark");
  report.checks.push(
    "Dark theme and responsive holding table render in native WebView2",
  );
  if (!process.argv.includes("--smoke")) {
    const first = await invoke("market_snapshot");
    await driver.findElement(By.css('a[href="/"]')).click();
    await sleep(65000);
    const background = await invoke("market_snapshot");
    const b = background.quotes.find((q) => q.instrumentId === "cg:bitcoin");
    const a = first.quotes.find((q) => q.instrumentId === "cg:bitcoin");
    assert.ok(b);
    assert.ok(!a || b.fetchedAt !== a.fetchedAt);
    report.checks.push(
      "Global native monitor continues while investment page is unmounted",
    );
    await driver.manage().window().minimize();
    const beforeMin = await invoke("market_snapshot");
    await sleep(65000);
    const min = await invoke("market_snapshot");
    assert.notEqual(
      min.quotes.find((q) => q.instrumentId === "cg:bitcoin")?.fetchedAt,
      beforeMin.quotes.find((q) => q.instrumentId === "cg:bitcoin")?.fetchedAt,
    );
    await driver.manage().window().setRect({ width: 1280, height: 860 });
    report.checks.push(
      "Quotes continue updating while native window is minimized",
    );
  }
  const backup = await invoke("create_backup");
  assert.ok((await fs.stat(backup)).size > 0);
  report.checks.push("Consistent backup includes investment ledger");
  await driver.quit();
  driver = null;
  await sleep(1500);
  const usageFile = path.join(
    folder,
    "ChengSuanData",
    "caches",
    "investment-usage.json",
  );
  const stopped = (await fs.stat(usageFile)).mtimeMs;
  await sleep(4000);
  assert.equal((await fs.stat(usageFile)).mtimeMs, stopped);
  report.checks.push("Native requests cease after closing the application");
  const moved = path.join(folder, "移动到中文路径");
  await fs.mkdir(moved);
  await fs.copyFile(executable, path.join(moved, "ChengSuan.exe"));
  await fs.rename(
    path.join(folder, "ChengSuanData"),
    path.join(moved, "ChengSuanData"),
  );
  driver = await session(path.join(moved, "ChengSuan.exe"));
  await driver.wait(until.elementLocated(By.css(".sidebar")), 20000);
  const restored = await invoke("investment_load");
  assert.deepEqual(restored.document, saved.document);
  report.checks.push(
    "Normal close, EXE replacement, data move and restart preserve exact investment document",
  );
  await shot("restart-moved");
} catch (error) {
  report.issues.push(error.stack ?? String(error));
  console.error(error);
  process.exitCode = 1;
} finally {
  if (driver) await driver.quit().catch(() => {});
  processDriver.kill();
  await log.close();
  report.finishedAt = new Date().toISOString();
  await fs.writeFile(
    path.join(folder, "report.json"),
    JSON.stringify(report, null, 2),
  );
  console.log(JSON.stringify(report, null, 2));
}
