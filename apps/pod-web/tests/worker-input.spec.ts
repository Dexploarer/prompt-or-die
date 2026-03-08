import { expect, test, type Page } from "@playwright/test";

type GameplayState = {
  renderThread: string;
  frameSource: string;
  focused: boolean;
  controlledEntityId: number | null;
  controlledPosition: [number, number] | null;
  selectedTargetId: number | null;
  clickMoveTarget: [number, number] | null;
  movementSignature: string;
  latestFeedback: string;
};

declare global {
  interface Window {
    podRender: {
      requestGameplayFocus: () => boolean;
      getGameplayState: () => GameplayState;
    };
  }
}

async function waitForGameplayReady(page: Page) {
  await page.waitForFunction(() => {
    return (
      typeof window.podRender?.getGameplayState === "function" &&
      window.podRender.getGameplayState().controlledPosition !== null
    );
  });
}

async function moveForward(page: Page) {
  await page.evaluate(() => window.podRender.requestGameplayFocus());
  await page.click("#pod-web-canvas");
  const before = await page.evaluate(() => window.podRender.getGameplayState());
  await page.keyboard.down("w");
  await page.waitForTimeout(700);
  await page.keyboard.up("w");
  await page.waitForTimeout(400);
  const after = await page.evaluate(() => window.podRender.getGameplayState());

  expect(before.focused).toBeTruthy();
  expect(before.controlledEntityId).not.toBeNull();
  expect(before.controlledPosition).not.toBeNull();
  expect(after.controlledPosition).not.toBeNull();
  const dx = after.controlledPosition![0] - before.controlledPosition![0];
  const dy = after.controlledPosition![1] - before.controlledPosition![1];
  expect(Math.hypot(dx, dy)).toBeGreaterThan(0.25);
}

test("main-thread route accepts gameplay focus and movement input", async ({ page }) => {
  await page.goto("/?backend=webgl2");
  await waitForGameplayReady(page);
  await moveForward(page);
});

test("worker route accepts gameplay focus and movement input", async ({ page }) => {
  await page.goto("/?renderThread=worker&backend=webgl2");
  await waitForGameplayReady(page);
  await moveForward(page);
});
