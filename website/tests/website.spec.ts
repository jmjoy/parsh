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
    "把 Shell",
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

  await page.keyboard.press("ArrowUp");
  await page.keyboard.type(" world)");
  await expect(terminal(page)).toContainText('"helloworld"');

  await page.keyboard.press("Control+p");
  await page.keyboard.press("Control+p");
  await page.keyboard.press("ArrowDown");
  await page.keyboard.type(" again)");
  await expect(terminal(page)).toContainText('"helloworldagain"');
});

test("bindings, closures, conditions, loops and Unicode retain shared semantics", async ({
  page,
}) => {
  await playground(page);
  await paste(
    page,
    `(let name outer)
(defn read-name () (str $name))
(defn caller (name) (read-name))
(caller inner)
(set name changed)
(caller inner)
(if (eq a a) (str "你好，parsh") (echo unavailable))
(for item (list first last) (str $item))`,
  );
  await expect(terminal(page)).toContainText('"outer"');
  await expect(terminal(page)).toContainText('"changed"');
  await expect(terminal(page)).toContainText('"你好，parsh"');
  await expect(terminal(page)).toContainText('"last"');
  await paste(page, "(export GREETING hello) (str $GREETING) (str name)");
  await expect(terminal(page)).toContainText('"hello"');
  await expect(terminal(page)).toContainText('"name"');
});

test("binding targets reject variable references and allow recovery", async ({
  page,
}) => {
  await playground(page);
  await paste(page, "(let name retained) (set $name changed)");
  await expect(terminal(page)).toContainText(
    "binding must use a bare variable name without '$'",
  );
  await paste(page, "(str $name)");
  await expect(terminal(page)).toContainText('"retained"');
});

test("paste recovers extra closers and a failed batch allows new input", async ({
  page,
}) => {
  await playground(page);
  await paste(page, "(str first))) (str second)");
  await expect(terminal(page)).toContainText('"second"');
  await expect(terminal(page)).toContainText("unexpected ')'");
  await paste(page, "(no-such-command) (str skipped)");
  await expect(terminal(page)).toContainText(
    "command 'no-such-command' is not available in this playground",
  );
  await expect(terminal(page)).not.toContainText('"skipped"');
  await paste(page, "(str recovered)");
  await expect(terminal(page)).toContainText('"recovered"');
});

test("unavailable commands produce source-tagged diagnostics and recover", async ({ page }) => {
  await playground(page);
  await paste(page, "(no-such-command)");
  await expect(terminal(page)).toContainText("interactive:1:1:");
  await expect(terminal(page)).toContainText("is not available in this playground");
  await page.getByRole("textbox", { name: "parsh terminal input" }).focus();
  await page.keyboard.press("Control+d");
  await expect(terminal(page)).toContainText("[exit 127]");
  await page.getByRole("button", { name: "Reset", exact: true }).click();
  await expect(page.locator("[data-playground]")).toHaveAttribute("data-state", "ready");
  await paste(page, "(echo recovered)");
  await expect(terminal(page)).toContainText("recovered");
});

test("clear and idle Ctrl+C preserve bindings; reset clears them", async ({
  page,
}) => {
  await playground(page);
  await paste(page, "(let name retained) (str $name)");
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
  await paste(page, `(let name before)\n${expensiveExpression()}`);
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


test("coreutils preserve Unicode argv, environment, nested captures, and raw output", async ({ page }) => {
  await playground(page);
  await paste(page, String.raw`(echo "你好，parsh 🐑") (export GREETING "你好 🐑") (printenv GREETING)`);
  await expect(terminal(page)).toContainText("你好，parsh 🐑");
  await expect(terminal(page)).toContainText("你好 🐑");
  await paste(page, String.raw`(defn capture () (printf "%s\n" "a value with spaces"))
(let value (capture))
(printf "<%s>\\n" $value)`);
  await expect(terminal(page)).toContainText("<a value with spaces>");
  await paste(page, String.raw`(printf "no-newline") (str complete) (| (printf "\\377\\000\\101") (base64))`);
  await expect(terminal(page)).toContainText("no-newline");
  await expect(terminal(page)).toContainText('"complete"');
  await expect(terminal(page)).toContainText("/wBB");
  await paste(page, String.raw`(printf "%0400d\n" 7)`);
  await expect(terminal(page)).toContainText("000007");
});

test("file commands, glob, cwd, and all redirection forms share temporary files", async ({ page }) => {
  await playground(page);
  await paste(page, String.raw`(mkdir -p demo) (cd demo)
(|> first.txt (printf "one\ntwo\n"))
(|1>> first.txt (echo three))
(cp first.txt copy.txt) (mv copy.txt moved.txt)
(touch empty.txt .hidden.txt) (str prepared)`);
  await expect(terminal(page)).toContainText('"prepared"');
  await paste(page, String.raw`(printf "glob:<%s>\\n" (glob *.txt))`);
  await expect(terminal(page)).toContainText("glob:<first.txt>");
  await expect(terminal(page)).toContainText("glob:<moved.txt>");
  await expect(terminal(page)).not.toContainText("glob:<.hidden.txt>");
  await paste(page, String.raw`(cat /home/playground/demo/first.txt) (cat ../numbers.txt)`);
  await expect(terminal(page)).toContainText("three");
  await expect(terminal(page)).toContainText("30");
  await paste(page, String.raw`(<| first.txt (sort -r))`);
  await expect(terminal(page)).toContainText("two");
  await paste(page, String.raw`(|2> errors.txt (if (cat missing.txt) (echo unexpected) (echo expected-failure)))`);
  await expect(terminal(page)).toContainText("expected-failure");
  await paste(page, String.raw`(|2>> errors.txt (if (cat another-missing.txt) (echo unexpected) (echo expected-again))) (cat errors.txt)`);
  await expect(terminal(page)).toContainText("expected-again");
  await expect(terminal(page)).toContainText("another-missing.txt: No such file");
  await paste(page, String.raw`(export PWD /bogus) (str (pwd)) (|> nested.txt (echo (pwd))) (cat nested.txt)`);
  await expect(terminal(page)).toContainText('"/home/playground/demo"');
  await paste(page, String.raw`(cd ..) (rm demo/empty.txt) (mkdir disposable) (rmdir disposable) (str (pwd))`);
  await expect(terminal(page)).toContainText('"/home/playground"');
});

test("pipelines preserve statuses and redirection file positions across commands", async ({ page }) => {
  await playground(page);
  await paste(page, String.raw`(| (sort fruits.txt) (uniq -c))`);
  await expect(terminal(page)).toContainText("2 apple");
  await paste(page, String.raw`(if (| (false) (true)) (echo wrong-branch) (echo right-branch))
(|1> out.txt (do (printf one) (printf two)))
(|>> out.txt (printf three)) (echo (cat out.txt))`);
  await expect(terminal(page)).toContainText("right-branch");
  await expect(terminal(page)).toContainText("onetwothree");
  await paste(page, String.raw`(|> binary.txt (printf "\\377\\000\\101")) (<| binary.txt (base64))`);
  await expect(terminal(page)).toContainText("/wBB");
  await paste(page, String.raw`(| (str invalid) (cat))`);
  await expect(terminal(page)).toContainText("v1 pipeline stages must be external commands");
  await paste(page, String.raw`(| (false) (true)) (str skipped-after-failure)`);
  await expect(terminal(page)).toContainText("false | true exited with status 1");
  await expect(terminal(page)).not.toContainText('"skipped-after-failure"');
});

test("clear preserves files while reset recreates the initial filesystem", async ({ page }) => {
  await playground(page);
  await paste(page, "(|> retained.txt (echo retained-file))");
  await page.getByRole("button", { name: "Clear", exact: true }).click();
  await paste(page, "(cat retained.txt)");
  await expect(terminal(page)).toContainText("retained-file");
  await page.getByRole("button", { name: "Reset", exact: true }).click();
  await expect(page.locator("[data-playground]")).toHaveAttribute("data-state", "ready");
  await paste(page, "(if (test -f retained.txt) (echo stale-file) (echo fresh-files)) (cat fruits.txt)");
  await expect(terminal(page)).toContainText("fresh-files");
  await expect(terminal(page)).toContainText("banana");
});

test("coreutils output and filesystem quotas fail without losing the session", async ({ page }) => {
  test.setTimeout(30_000);
  await playground(page);
  await paste(page, "(let retained yes) (seq 1 2000000)");
  await expect(terminal(page)).toContainText("exceeds 8 MiB");
  await paste(page, "(str $retained)");
  await expect(terminal(page)).toContainText('"yes"');
  await paste(page, '(|> chunk.txt (seq 1 600000)) (str chunk-ready)');
  await expect(terminal(page)).toContainText('"chunk-ready"');
  await paste(page, Array.from({ length: 8 }, (_, index) => `(cp chunk.txt copy${index}.txt)`).join(" "));
  await expect(terminal(page)).toContainText("virtual files exceed 32 MiB", { timeout: 15_000 });
  await paste(page, "(echo after-limit)");
  await expect(terminal(page)).toContainText("after-limit");
});

for (const asset of ["coreutils.wasm", "coreutils.json"]) {
  test(`a failed ${asset} load exposes a working retry`, async ({ page }) => {
    await page.route(`**/wasm/${asset}`, route => route.abort());
    await page.goto("/parsh/en/playground/");
    await expect(page.locator("[data-playground]")).toHaveAttribute("data-state", "error");
    await page.unroute(`**/wasm/${asset}`);
    await page.getByRole("button", { name: "Retry", exact: true }).click();
    await expect(page.locator("[data-playground]")).toHaveAttribute("data-state", "ready");
    await paste(page, "(echo retried-command)");
    await expect(terminal(page)).toContainText("retried-command");
  });
}

for (const locale of ["en", "zh-cn"]) {
  test(`coreutils example buttons run in ${locale}`, async ({ page }) => {
    await playground(page, locale);
    for (const [example, output] of [["commands", "你好，parsh"], ["pipeline", "2 apple"], ["files", "hello"], ["capture", "found fruits.txt"]]) {
      await page.locator(`[data-example="${example}"]`).click();
      await expect(terminal(page)).toContainText(output!);
    }
  });
}

test("Tab completes browser commands, temporary files, and session variables", async ({ page }) => {
  await playground(page);
  await page.getByRole("textbox", { name: "parsh terminal input" }).focus();
  await page.keyboard.type("ec");
  await page.keyboard.press("Tab");
  await expect(terminal(page)).toContainText("(echo");
  await page.keyboard.type("BROWSER_TAB_COMMAND)");
  await expect.poll(async () =>
    (await terminal(page).innerText()).split("\n").some(line => line.trim() === "BROWSER_TAB_COMMAND"),
  ).toBe(true);
  await page.keyboard.type("cat fru");
  await page.keyboard.press("Tab");
  await expect(terminal(page)).toContainText("(cat fruits.txt");
  await expect(terminal(page)).not.toContainText("banana");
  await page.keyboard.type(")");
  await expect(terminal(page)).toContainText("banana");
  await paste(page, "(let TAB_VALUE retained)");
  await page.keyboard.type("str $TAB_V");
  await page.keyboard.press("Tab");
  await expect(terminal(page)).toContainText("(str $TAB_VALUE");
  await page.keyboard.type(")");
  await expect(terminal(page)).toContainText('"retained"');
});

test("double Tab lists candidates and preserves the prompt and further editing", async ({ page }) => {
  await playground(page);
  await paste(page, "(defn tab-alpha () (str TAB_ALPHA)) (defn tab-alpine () (str TAB_ALPINE))");
  await page.getByRole("textbox", { name: "parsh terminal input" }).focus();
  await page.keyboard.type("tab-a");
  await page.keyboard.press("Tab");
  await expect(terminal(page)).toContainText("(tab-alp");
  await page.keyboard.press("Tab");
  await expect.poll(async () =>
    (await terminal(page).innerText()).split("\n").filter(line => /^tab-alp(ha|ine)$/.test(line.trim())),
  ).toEqual(["tab-alpha", "tab-alpine"]);
  await page.keyboard.type("h");
  await page.keyboard.press("Tab");
  await page.keyboard.type(")");
  await expect(terminal(page)).toContainText('"TAB_ALPHA"');
  await page.keyboard.type("(; ec");
  await page.keyboard.press("Tab");
  await page.keyboard.type(")");
  await expect(terminal(page)).toContainText("(; ec)");
});

test("Tab quotes Unicode paths and follows directories in the narrow Chinese terminal", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await playground(page, "zh-cn");
  await paste(page, '(mkdir "folder name") (touch "folder name/你好 file.txt")');
  await page.getByRole("textbox", { name: "parsh 终端输入" }).focus();
  await page.keyboard.type("str fol");
  await page.keyboard.press("Tab");
  await expect(terminal(page)).toContainText('"folder name/');
  await page.keyboard.press("Tab");
  await page.keyboard.type(")");
  await expect(terminal(page)).toContainText('"folder name/你好 file.txt"');
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
});
