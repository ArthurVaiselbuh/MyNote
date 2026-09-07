import { expect, sleep, test, type App } from "../app";

type TauriInternals = {
  invoke(command: string, args?: Record<string, unknown>, options?: unknown): Promise<unknown>;
};

type E2eWindow = typeof window & {
  __TAURI_INTERNALS__: TauriInternals;
  __mynoteE2eOriginalFetch?: PropertyDescriptor;
  __mynoteE2eInterceptedCalls?: number;
  __mynoteE2eWriteCalls?: number;
  __mynoteE2eReadStarted?: boolean;
};

function restorePageFetch(app: App) {
  return app.page.evaluate(() => {
    const testWindow = window as E2eWindow;
    const descriptor = testWindow.__mynoteE2eOriginalFetch;
    if (descriptor) {
      Object.defineProperty(testWindow, "fetch", descriptor);
    } else {
      delete testWindow.fetch;
    }
    delete testWindow.__mynoteE2eOriginalFetch;
  });
}

type InvokeInterception =
  | { kind: "reject-write" }
  | { kind: "delay-write"; delay: number }
  | { kind: "reject-read"; pageId: string }
  | { kind: "delay-read"; pageId: string; delay: number };

function interceptPageFetch(app: App, interception: InvokeInterception) {
  return app.page.evaluate((interception) => {
    const testWindow = window as E2eWindow;
    const descriptor = Object.getOwnPropertyDescriptor(testWindow, "fetch");
    if (descriptor && !descriptor.writable && !descriptor.configurable) {
      throw new Error("window.fetch cannot be intercepted");
    }
    const fetch = testWindow.fetch.bind(testWindow);
    let intercepted = false;
    testWindow.__mynoteE2eInterceptedCalls = 0;
    testWindow.__mynoteE2eWriteCalls = 0;
    testWindow.__mynoteE2eReadStarted = false;
    testWindow.fetch = ((input, init) => {
      const url = input instanceof Request ? input.url : input.toString();
      const command = new URL(url).hostname === "ipc.localhost"
        ? decodeURIComponent(new URL(url).pathname.slice(1))
        : "";
      const payload = typeof init?.body === "string" ? JSON.parse(init.body) as { id?: string } : {};
      const matchesPageRead =
        (interception.kind === "reject-read" || interception.kind === "delay-read") &&
        command === "read_page" &&
        payload.id === interception.pageId;
      const matches =
        ((interception.kind === "reject-write" || interception.kind === "delay-write") &&
          command === "write_page") ||
        matchesPageRead;
      const oneShot = interception.kind !== "reject-write";
      if (!matches || (oneShot && intercepted)) return fetch(input, init);
      intercepted = true;
      testWindow.__mynoteE2eInterceptedCalls!++;
      if (interception.kind === "reject-write") {
        return Promise.resolve(new Response("injected page write failure", {
          headers: { "Tauri-Response": "error", "content-type": "text/plain" },
        }));
      }
      if (interception.kind === "reject-read") {
        return Promise.resolve(new Response("injected page read failure", {
          headers: { "Tauri-Response": "error", "content-type": "text/plain" },
        }));
      }
      if (interception.kind === "delay-write") {
        testWindow.__mynoteE2eWriteCalls!++;
      } else {
        testWindow.__mynoteE2eReadStarted = true;
      }
      return new Promise((resolve) => setTimeout(resolve, interception.delay)).then(() =>
        fetch(input, init),
      );
    }) as typeof window.fetch;
    testWindow.__mynoteE2eOriginalFetch = descriptor;
  }, interception);
}

function rejectFirstPageRead(app: App, pageId: string) {
  return interceptPageFetch(app, { kind: "reject-read", pageId });
}

function rejectPageWrites(app: App) {
  return interceptPageFetch(app, { kind: "reject-write" });
}

function delayPageWrites(app: App, delay: number) {
  return interceptPageFetch(app, { kind: "delay-write", delay });
}

function delayFirstPageRead(app: App, pageId: string, delay: number) {
  return interceptPageFetch(app, { kind: "delay-read", pageId, delay });
}

test("a failed save keeps the open page and editor buffer in place", async ({ app }) => {
  await app.newTitledPage("First", 1);
  const [first] = await app.treeIds();
  await app.newTitledPage("Second", 2);

  await app.openPage("First");
  await app.selectWholeBody();
  await app.page.keyboard.insertText("unsaved first-page body");
  await expect(app.editorBody).toContainText("unsaved first-page body");
  await rejectPageWrites(app);
  try {
    await app.page.keyboard.press("Control+s");
    await expect.poll(() => app.page.evaluate(() => (window as E2eWindow).__mynoteE2eInterceptedCalls)).toBe(1);
    await expect(app.page.locator(".status-toast")).toContainText("injected page write failure");

    await app.row("Second").click();
    await sleep(100);
    await expect(app.selectedTitle).toHaveText("First");
    await expect(app.editorBody).toContainText("unsaved first-page body");
    expect(app.readMd(first)).not.toContain("unsaved first-page body");
  } finally {
    await restorePageFetch(app);
  }
});

test("a failed page read cannot redirect the next save into that page", async ({ app }) => {
  await app.newTitledPage("First", 1);
  const [first] = await app.treeIds();
  await app.setBody("first on disk");
  await app.newTitledPage("Second", 2);
  const [, second] = await app.treeIds();
  await app.setBody("second on disk");

  await app.openPage("First");
  await expect(app.editorBody).toContainText("first on disk");
  await rejectFirstPageRead(app, second);
  try {
    await app.row("Second").click();
    await expect.poll(() => app.page.evaluate(() => (window as E2eWindow).__mynoteE2eInterceptedCalls)).toBe(1);
    await expect(app.selectedTitle).toHaveText("First");
    await expect(app.editorBody).toContainText("first on disk");
  } finally {
    await restorePageFetch(app);
  }

  await app.selectWholeBody();
  await app.page.keyboard.insertText("first after failed read");
  await app.page.keyboard.press("Control+s");
  await expect.poll(() => app.readMd(first)).toContain("first after failed read");
  expect(app.readMd(second)).not.toContain("first after failed read");
});

test("a pending save drains edits made while page navigation waits", async ({ app }) => {
  await app.newTitledPage("First", 1);
  const [first] = await app.treeIds();
  await app.newTitledPage("Second", 2);
  await app.setBody("second page body");
  await app.openPage("First");
  await app.selectWholeBody();
  await app.page.keyboard.insertText("first revision");
  await delayPageWrites(app, 800);

  await app.page.keyboard.press("Control+s");
  await app.page.waitForFunction(() => (window as E2eWindow).__mynoteE2eWriteCalls === 1);
  await app.row("Second").click();
  await app.editorBody.click();
  await app.page.keyboard.insertText(" and second revision");

  try {
    await expect(app.selectedTitle).toHaveText("Second");
    await expect(app.editorBody).toContainText("second page body");
    await expect.poll(() => app.readMd(first)).toContain("first revision and second revision");
  } finally {
    await restorePageFetch(app);
  }
});

test("a delayed page read follows a successful save of the page being left", async ({ app }) => {
  await app.newTitledPage("First", 1);
  const [first] = await app.treeIds();
  await app.newTitledPage("Second", 2);
  const [, second] = await app.treeIds();
  await app.setBody("second page body");

  await app.row("First").click();
  await app.selectWholeBody();
  await app.page.keyboard.insertText("first edit before delayed read");
  await delayFirstPageRead(app, second, 800);
  try {
    await app.row("Second").click();
    await app.page.waitForFunction(() => (window as E2eWindow).__mynoteE2eReadStarted === true);
    await expect.poll(() => app.page.evaluate(() => (window as E2eWindow).__mynoteE2eInterceptedCalls)).toBe(1);
    await app.editorBody.click();
    await app.page.keyboard.insertText(" and edit during delayed read");
    await expect(app.selectedTitle).toHaveText("Second");
    await expect(app.editorBody).toContainText("second page body");
    await expect
      .poll(() => app.readMd(first))
      .toContain("first edit before delayed read and edit during delayed read");
  } finally {
    await restorePageFetch(app);
  }
});

test("a failed close save keeps the window open and the next close persists", async ({ app }) => {
  await app.newTitledPage("Close failure", 1);
  const [id] = await app.treeIds();
  await app.selectWholeBody();
  await app.page.keyboard.insertText("must remain in the editor");
  await rejectPageWrites(app);
  try {
    await app.page.evaluate(() =>
      (window as E2eWindow).__TAURI_INTERNALS__.invoke("plugin:window|close", { label: "main" }),
    );
    await expect.poll(() => app.page.evaluate(() => (window as E2eWindow).__mynoteE2eInterceptedCalls)).toBe(1);
    await expect(app.page.locator(".modal-title")).toHaveText("Couldn't save");
    await expect(app.page.locator(".modal")).toContainText("Close failure");
    await app.page.getByRole("button", { name: "Keep editing" }).click();
    await expect(app.editorBody).toContainText("must remain in the editor");
  } finally {
    await restorePageFetch(app);
  }
  await app.close();
  await expect.poll(() => app.readMd(id)).toContain("must remain in the editor");
});

test("window close waits for delayed save completion", async ({ app }) => {
  await app.newTitledPage("Slow close", 1);
  const [id] = await app.treeIds();
  await app.selectWholeBody();
  await app.page.keyboard.insertText("persist after delayed close");
  await delayPageWrites(app, 2_000);

  await app.close();
  await expect.poll(() => app.readMd(id)).toContain("persist after delayed close");
});
