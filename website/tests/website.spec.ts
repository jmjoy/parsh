import { test, expect, type Page } from "@playwright/test";

const terminal = (page: Page) => page.locator(".xterm-accessibility-tree");
async function playground(page: Page, locale = "en") {
  await page.goto(`/parsh/${locale}/playground/`);
  await expect(page.locator("[data-playground]")).toHaveAttribute(
    "data-state",
    "ready",
  );
}
async function paste(page: Page, source: string) {
  await page.locator("[data-terminal-host]").evaluate((host, text) => {
    const clipboardData = new DataTransfer();
    clipboardData.setData("text/plain", text);
    host.dispatchEvent(
      new ClipboardEvent("paste", {
        clipboardData,
        bubbles: true,
        cancelable: true,
      }),
    );
  }, source);
}
function expensiveExpression() {
  return (
    "(defn work0 () (str done))\n" +
    Array.from(
      { length: 30 },
      (_, index) =>
        `(defn work${index + 1} () (do (work${index}) (work${index})))`,
    ).join("\n") +
    "\n(work30)"
  );
}

test("root redirects to prefixed English and both homepages share navigation", async ({
  page,
}) => {
  await page.goto("/parsh/");
  await expect(page).toHaveURL(/\/parsh\/en\/$/);
  await expect(page.getByRole("heading", { level: 1 })).toContainText(
    "Your shell",
  );
  await page.getByRole("link", { name: "切换到简体中文" }).click();
  await expect(page).toHaveURL(/\/parsh\/zh-cn\/$/);
  await expect(page.getByRole("heading", { level: 1 })).toContainText(
    "你的 Shell",
  );
  await expect(
    page
      .getByRole("navigation", { name: "主导航" })
      .getByRole("link", { name: "文档", exact: true }),
  ).toHaveAttribute("href", "/parsh/zh-cn/docs/");
});

test("theme persists between the homepage, playground, and docs", async ({
  page,
}) => {
  await page.goto("/parsh/en/");
  const oldTheme = await page.locator("html").getAttribute("data-theme");
  await page.getByRole("button", { name: "Toggle color theme" }).click();
  const theme = oldTheme === "dark" ? "light" : "dark";
  await expect(page.locator("html")).toHaveAttribute("data-theme", theme);
  await page.goto("/parsh/en/docs/");
  await expect(page.locator("html")).toHaveAttribute("data-theme", theme);
  await playground(page);
  await expect(page.locator("html")).toHaveAttribute("data-theme", theme);
});

test("WASM loads from the project subpath and executes real expressions", async ({
  page,
}) => {
  const wasm = page.waitForResponse((response) =>
    response.url().endsWith("/parsh/wasm/parsh_bg.wasm"),
  );
  await playground(page);
  expect((await wasm).status()).toBe(200);
  await page.getByRole("button", { name: /Hello, parsh/ }).click();
  await expect(terminal(page)).toContainText('"hello, parsh"');
  await expect(terminal(page)).toContainText(
    '("text" "a value with spaces" true)',
  );
});

test("native submission keys, multiline editing, and history work in the browser", async ({
  page,
}) => {
  await playground(page);
  const input = page.getByRole("textbox", { name: "parsh terminal input" });
  await input.focus();
  await page.keyboard.type("str hello");
  await page.keyboard.press("Enter");
  await expect(terminal(page)).not.toContainText('"hello"');
  await page.keyboard.type(")");
  await expect(terminal(page)).toContainText('"hello"');
  await page.keyboard.press("Control+p");
  await page.keyboard.type(")");
  await expect
    .poll(
      async () =>
        (await terminal(page).innerText()).split('"hello"').length - 1,
    )
    .toBe(2);
});

test("bindings, closures, conditions, loops and Unicode retain shared semantics", async ({
  page,
}) => {
  await playground(page);
  await paste(
    page,
    `(let $name outer)
(defn read-name () (str $name))
(defn caller ($name) (read-name))
(caller inner)
(set $name changed)
(caller inner)
(if (eq a a) (str "你好，parsh") (echo unavailable))
(for $item (list first last) (str $item))`,
  );
  await expect(terminal(page)).toContainText('"outer"');
  await expect(terminal(page)).toContainText('"changed"');
  await expect(terminal(page)).toContainText('"你好，parsh"');
  await expect(terminal(page)).toContainText('"last"');
});

test("paste recovers extra closers and a failed batch allows new input", async ({
  page,
}) => {
  await playground(page);
  await paste(page, "(str first))) (str second)");
  await expect(terminal(page)).toContainText('"second"');
  await expect(terminal(page)).toContainText("unexpected ')'");
  await paste(page, "(echo unavailable) (str skipped)");
  await expect(terminal(page)).toContainText(
    "external command 'echo' requires native parsh",
  );
  await expect(terminal(page)).not.toContainText('"skipped"');
  await paste(page, "(str recovered)");
  await expect(terminal(page)).toContainText('"recovered"');
});

test("native-only forms produce source-tagged diagnostics", async ({
  page,
}) => {
  await playground(page);
  for (const source of [
    "(cd /)",
    "(glob *)",
    "(| (cat) (sort))",
    "(|> file.txt (str text))",
  ]) {
    await paste(page, source);
    await expect(terminal(page)).toContainText("interactive:1:1:");
    await expect(terminal(page)).toContainText("requires native parsh");
  }
});

test("clear and idle Ctrl+C preserve bindings; reset clears them", async ({
  page,
}) => {
  await playground(page);
  await paste(page, "(let $name retained) (str $name)");
  await expect(terminal(page)).toContainText('"retained"');
  await page.getByRole("button", { name: "Clear", exact: true }).click();
  await expect(terminal(page)).not.toContainText('"retained"');
  const input = page.getByRole("textbox", { name: "parsh terminal input" });
  await input.focus();
  await page.keyboard.type("str unfinished");
  await page.keyboard.press("Control+c");
  await paste(page, "(str $name)");
  await expect(terminal(page)).toContainText('"retained"');
  await page.getByRole("button", { name: "Reset", exact: true }).click();
  await expect(page.locator("[data-playground]")).toHaveAttribute(
    "data-state",
    "ready",
  );
  await paste(page, "(str $name)");
  await expect(terminal(page)).toContainText("undefined variable $name");
});

test("running Ctrl+C stops execution and resets the session", async ({
  page,
}) => {
  await playground(page);
  await page.getByRole("textbox", { name: "parsh terminal input" }).focus();
  await paste(page, `(let $name before)\n${expensiveExpression()}`);
  await expect(page.locator("[data-playground]")).toHaveAttribute(
    "data-state",
    "running",
  );
  await page.keyboard.press("Control+c");
  await expect(page.locator("[data-playground]")).toHaveAttribute(
    "data-state",
    "ready",
  );
  await expect(page.locator("[data-notice]")).toContainText(
    "Execution interrupted",
  );
  await paste(page, "(str $name)");
  await expect(terminal(page)).toContainText("undefined variable $name");
});

test("long-running expressions time out without freezing the page", async ({
  page,
}) => {
  test.setTimeout(20_000);
  await playground(page);
  await paste(page, expensiveExpression());
  await expect(page.locator("[data-notice]")).toContainText(
    "Execution exceeded 5 seconds",
    { timeout: 12_000 },
  );
  await expect(page.locator("[data-playground]")).toHaveAttribute(
    "data-state",
    "ready",
  );
  await paste(page, "(str responsive)");
  await expect(terminal(page)).toContainText('"responsive"');
});

test("exit and Ctrl+D end a session until reset", async ({ page }) => {
  await playground(page);
  await paste(page, "(exit 7)");
  await expect(page.locator("[data-playground]")).toHaveAttribute(
    "data-state",
    "closed",
  );
  await expect(terminal(page)).toContainText("[exit 7]");
  await page.getByRole("button", { name: "Reset", exact: true }).click();
  await expect(page.locator("[data-playground]")).toHaveAttribute(
    "data-state",
    "ready",
  );
  await page.getByRole("textbox", { name: "parsh terminal input" }).focus();
  await page.keyboard.press("Control+d");
  await expect(page.locator("[data-playground]")).toHaveAttribute(
    "data-state",
    "closed",
  );
});

test("a failed WASM load exposes a working retry action", async ({ page }) => {
  await page.route("**/wasm/parsh_bg.wasm", (route) => route.abort());
  await page.goto("/parsh/en/playground/");
  await expect(page.locator("[data-playground]")).toHaveAttribute(
    "data-state",
    "error",
  );
  await page.unroute("**/wasm/parsh_bg.wasm");
  await page.getByRole("button", { name: "Retry", exact: true }).click();
  await expect(page.locator("[data-playground]")).toHaveAttribute(
    "data-state",
    "ready",
  );
  await paste(page, "(str retried)");
  await expect(terminal(page)).toContainText('"retried"');
});

test("both documentation trees load directly and search the built index", async ({
  page,
}) => {
  for (const locale of ["en", "zh-cn"]) {
    await page.goto(`/parsh/${locale}/docs/control-flow/`);
    await expect(page.getByRole("heading", { level: 1 })).toContainText(
      locale === "en" ? "Functions" : "函数",
    );
    const links = page.getByRole("navigation", {
      name: locale === "en" ? "Main navigation" : "主导航",
    });
    await expect(
      links.getByRole("link", {
        name: locale === "en" ? "Playground" : "体验终端",
      }),
    ).toHaveAttribute("href", `/parsh/${locale}/playground/`);
  }
  await page.goto("/parsh/en/docs/");
  await page.getByRole("button", { name: /Search/ }).click();
  await page
    .getByRole("textbox", { name: "Search documentation" })
    .fill("lexical");
  await expect(
    page
      .getByRole("dialog", { name: "Search documentation" })
      .getByRole("link", { name: "Variables & scope", exact: true })
      .first(),
  ).toBeVisible();
});

test("Chinese terminal and narrow layouts remain usable", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await playground(page, "zh-cn");
  await page.getByRole("button", { name: /你好，parsh/ }).click();
  await expect(terminal(page)).toContainText('"hello, parsh"');
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth,
    ),
  ).toBe(true);
  await page.goto("/parsh/zh-cn/");
  await expect(page.getByRole("heading", { level: 1 })).toBeVisible();
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth,
    ),
  ).toBe(true);
});


test("missing pages provide working links to both locale homepages", async ({ page }) => {
  const response = await page.goto("/parsh/missing-page/");
  expect(response?.status()).toBe(404);
  await expect(page.getByRole("heading", { level: 1 })).toContainText("Nothing inside");
  await expect(page.getByRole("link", { name: "切换到简体中文" })).toHaveAttribute("href", "/parsh/zh-cn/");
  await expect(page.getByRole("link", { name: "Back to home" })).toHaveAttribute("href", "/parsh/en/");
});
