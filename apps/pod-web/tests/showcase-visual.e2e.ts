import { expect, test, type Page } from "@playwright/test";

test.setTimeout(45_000);

type GameplayState = {
  frameSource: string;
  worldMode: string | null;
  worldName: string | null;
  controlledPosition: [number, number] | null;
};

declare global {
  interface Window {
    advanceTime: (ms: number) => Promise<void>;
    podRender: {
      getGameplayState: () => GameplayState;
    };
  }
}

async function waitForShowcaseReady(page: Page) {
  await page.waitForFunction(() => {
    return (
      typeof window.advanceTime === "function" &&
      typeof window.podRender?.getGameplayState === "function"
    );
  });

  for (let step = 0; step < 24; step += 1) {
    const ready = await page.evaluate(() => {
      const state = window.podRender.getGameplayState();
      return (
        state.worldMode === "bootstrap-showcase" &&
        state.worldName === "Resonant Shore" &&
        state.controlledPosition !== null &&
        state.frameSource === "threejs"
      );
    });

    if (ready) {
      return;
    }

    await page.evaluate(() => window.advanceTime(250));
  }

  await page.waitForFunction(() => {
    const state = window.podRender.getGameplayState();
    return (
      state.worldMode === "bootstrap-showcase" &&
      state.worldName === "Resonant Shore" &&
      state.controlledPosition !== null &&
      state.frameSource === "threejs"
    );
  });
}

test("bootstrap showcase intro framing remains visually stable", async ({ page }) => {
  await page.goto("/?world=bootstrap-showcase&backend=webgl2&fixedTimeMs=0&paused=1");
  await waitForShowcaseReady(page);
  const canvas = page.locator("#pod-web-canvas");
  const box = await canvas.boundingBox();

  expect(box).not.toBeNull();
  const screenshot = await page.screenshot({
    animations: "disabled",
    caret: "hide",
    clip: {
      x: box!.x,
      y: box!.y,
      width: box!.width,
      height: box!.height
    }
  });

  expect(screenshot).toMatchSnapshot("bootstrap-showcase-intro.png", {
    maxDiffPixelRatio: 0.01
  });
});
