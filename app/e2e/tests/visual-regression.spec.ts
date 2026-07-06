import { test, expect } from "@playwright/test";

const WALLET_ADDR = "GDEPOSITSOMERANDOMSTELLARADDRESS1234567890";

async function setupFreighter(page: any) {
  await page.addInitScript((addr: string) => {
    (window as any).freighterApi = {
      getAddress: () => Promise.resolve(addr),
      getPublicKey: () => Promise.resolve(addr),
      requestAccess: () => Promise.resolve(),
      setAllowed: () => Promise.resolve(),
      signMessage: (msg: string) => Promise.resolve("0x" + btoa(msg)),
    };
  }, WALLET_ADDR);
}

async function connectFreighter(page: any) {
  await setupFreighter(page);
  await page.goto("/");
  await page.getByText("CLICK ANYWHERE TO START").click();
  await page.waitForTimeout(500);
  await page.getByText("CONNECT FREIGHTER").click();
  await page.waitForTimeout(1000);
}

test.describe("visual regression: lobby", () => {
  test("lobby main menu screenshot", async ({ page }) => {
    await connectFreighter(page);
    await expect(page.getByText("MAIN MENU")).toBeVisible();
    await page.waitForTimeout(500);
    await expect(page).toHaveScreenshot("lobby-main-menu.png", {
      maxDiffPixelRatio: 0.02,
    });
  });

  test("lobby connect screen screenshot", async ({ page }) => {
    await page.goto("/");
    await page.getByText("CLICK ANYWHERE TO START").click();
    await page.waitForTimeout(1000);
    await expect(page).toHaveScreenshot("lobby-connect-screen.png", {
      maxDiffPixelRatio: 0.02,
    });
  });
});

test.describe("visual regression: table", () => {
  test("table pre-deal screenshot", async ({ page }) => {
    await connectFreighter(page);
    await page.getByText("CREATE TABLE").click();
    await page.waitForTimeout(500);
    await expect(page.getByText("CREATE A TABLE")).toBeVisible();
    await page.waitForTimeout(500);
    await expect(page).toHaveScreenshot("table-pre-deal.png", {
      maxDiffPixelRatio: 0.02,
    });
  });

  test("table mid-hand screenshot", async ({ page }) => {
    await connectFreighter(page);
    await page.getByText("CREATE TABLE").click();
    await page.waitForTimeout(500);
    await page.getByRole("button", { name: /solo/i }).click();
    await page.waitForTimeout(1500);
    await expect(page).toHaveScreenshot("table-mid-hand.png", {
      maxDiffPixelRatio: 0.02,
    });
  });

  test("showdown screenshot", async ({ page }) => {
    await connectFreighter(page);
    await page.getByText("CREATE TABLE").click();
    await page.waitForTimeout(500);
    if (page.getByRole("button", { name: /solo/i })) {
      await page.getByRole("button", { name: /solo/i }).click();
    }
    await page.waitForTimeout(2000);
    await expect(page).toHaveScreenshot("table-showdown.png", {
      maxDiffPixelRatio: 0.02,
    });
  });
});

test.describe("visual regression: proof explorer", () => {
  test("proof explorer open screenshot", async ({ page }) => {
    await connectFreighter(page);
    await page.getByText("CREATE TABLE").click();
    await page.waitForTimeout(500);
    if (page.getByRole("button", { name: /solo/i })) {
      await page.getByRole("button", { name: /solo/i }).click();
    }
    await page.waitForTimeout(1500);
    const gameboyButton = page.getByRole("button", { name: /game|menu|proof/i });
    if (await gameboyButton.isVisible().catch(() => false)) {
      await gameboyButton.click();
      await page.waitForTimeout(500);
    }
    await expect(page).toHaveScreenshot("proof-explorer-open.png", {
      maxDiffPixelRatio: 0.02,
    });
  });
});

test.describe("visual regression: mobile viewport", () => {
  test.use({ viewport: { width: 375, height: 667 } });

  test("mobile lobby screenshot", async ({ page }) => {
    await connectFreighter(page);
    await expect(page.getByText("MAIN MENU")).toBeVisible();
    await page.waitForTimeout(500);
    await expect(page).toHaveScreenshot("mobile-lobby.png", {
      maxDiffPixelRatio: 0.02,
    });
  });

  test("mobile table screenshot", async ({ page }) => {
    await connectFreighter(page);
    await page.getByText("CREATE TABLE").click();
    await page.waitForTimeout(500);
    await expect(page).toHaveScreenshot("mobile-table.png", {
      maxDiffPixelRatio: 0.02,
    });
  });
});
