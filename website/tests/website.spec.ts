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
async function execute(page: Page, source: string) {
  await paste(page, source);
  await page.locator("[data-terminal-host] textarea").focus();
  await page.keyboard.press("Enter");
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

test("maps preserve insertion order, value types, and eager defaults in the browser", async ({ page }) => {
  await playground(page);
  await execute(page, `
    (let config (map name parsh count (+ 1) nested (map enabled (eq a a))))
    (let copy $config)
    (let updated (mset $config name changed-copy))
    (let deleted (mdel $config count))
    (list (mget $config name) (mget $updated name) (mhas $config count) (mhas $deleted count))
    (set config (mset $config name updated))
    (set config (mdel $config count))
    (set config (mset $config count (+ 2)))
    (list (mkeys $config) (mget $config count) (mlen $config)
          (mhas $config missing) (mget $copy name))
    (let defaults (+))
    (mget $config nested (do (set defaults (+ $defaults 1)) fallback))
    (list $defaults)
    (mget $config missing (do (set config (mset $config fallback added)) (mget $config fallback)))
    (mvals $config)
    (mget (mset {name literal} name composed) name)
  `);
  await expect(terminal(page)).toContainText('("parsh" "changed-copy" true false)');
  await expect(terminal(page)).toContainText('(("name" "nested" "count") 2 3 false "parsh")');
  await expect(terminal(page)).toContainText('(map "enabled" true)');
  await expect(terminal(page)).toContainText('(1)');
  await expect(terminal(page)).toContainText('"added"');
  await expect(terminal(page)).toContainText('("updated" (map "enabled" true) 2 "added")');
  await expect(terminal(page)).toContainText('"composed"');

  await execute(page, '(map duplicate first duplicate (exit 9))');
  await expect(terminal(page)).toContainText('duplicate map key "duplicate"');
  await execute(page, '(str "recovered:" (mget $config name))');
  await expect(terminal(page)).toContainText('"recovered:updated"');
  await expect(page.locator("[data-playground]")).toHaveAttribute("data-state", "ready");
});

test("function values, optional and rest parameters, and the standard library share WASM semantics", async ({ page }) => {
  await playground(page);
  await execute(page, `
    (defn counter (n)
      (fn (&optional (step 1) &rest extra)
        (set n (+ $n $step))
        (for amount $extra (set n (+ $n $amount)))
        (return $n)))
    (let next (counter 10))
    (list ($next) ($next 2 3) (apply $next []))
    (let original (ssplit "a,b" ","))
    (list $original (lappend $original [c d]) (sjoin $original "/"))
    (list (slen "中文") (sslice "中文abc" 1 3) (quot -7 3) (rem -7 3) (round -2.5))
    (defmacro when (condition &rest body) \`(if ,$condition (do ,@$body)))
    (defn find (items) (for item $items (when (eq $item b) (return $item))) (str missing))
    (find [a b c])
    (list (apply (function max) [2 3 1]) (function? $next) (typeof (+ 1)))
  `);
  await expect(terminal(page)).toContainText("(11 16 17)");
  await expect(terminal(page)).toContainText('(("a" "b") ("a" "b" ("c" "d")) "a/b")');
  await expect(terminal(page)).toContainText('(2 "文a" -2 -1 -3)');
  await expect(terminal(page)).toContainText('"b"');
  await expect(terminal(page)).toContainText('(3 true "int")');
  await execute(page, '(mget {key value} key $undefined)');
  await expect(terminal(page)).toContainText('undefined variable $undefined');
  await execute(page, '(str recovered-functions)');
  await expect(terminal(page)).toContainText('"recovered-functions"');
});

test("collection shorthand supports nested values, Enter submission, and history in the browser", async ({ page }) => {
  await playground(page);
  await execute(page, `
    []{}
    (let config {name parsh items [one "two words"] nested {enabled (eq a a)}})
    [(mget $config items) (mget $config nested)]
    (eq [one {key [two]}] (list one (map key (list two))))
  `);
  await expect(terminal(page)).toContainText('(("one" "two words") (map "enabled" true))');
  await expect(terminal(page)).toContainText("true");

  await page.keyboard.type("[first");
  await expect(terminal(page)).toContainText("$ [first]");
  await page.keyboard.press("Enter");
  await expect(terminal(page)).toContainText('("first")');
  await page.keyboard.press("Control+p");
  await page.keyboard.type(" second");
  await page.keyboard.press("Enter");
  await expect(terminal(page)).toContainText('("first" "second")');

  await page.keyboard.type("{key value");
  await expect(terminal(page)).toContainText("$ {key value}");
  await page.keyboard.press("Enter");
  await expect(terminal(page)).toContainText('(map "key" "value")');
  await execute(page, "{duplicate first duplicate (exit 9)}");
  await expect(terminal(page)).toContainText('duplicate map key "duplicate"');
  await execute(page, "[recovered]");
  await expect(terminal(page)).toContainText('("recovered")');
});

test("Enter submission, forced multiline editing, and complete history work in the browser", async ({ page }) => {
  await playground(page);
  await expect(terminal(page)).toContainText("$ ()");
  await page.getByRole("textbox", { name: "parsh terminal input" }).focus();
  await page.keyboard.type("str hello)");
  await expect(terminal(page)).not.toContainText('"hello"');
  await page.keyboard.press("Enter");
  await expect(terminal(page)).toContainText('"hello"');
  await page.keyboard.press("Control+p");
  await expect(terminal(page)).toContainText("(str hello)");
  await page.keyboard.press("Enter");
  await expect.poll(async () => (await terminal(page).innerText()).split('"hello"').length - 1).toBe(2);
  await page.keyboard.press("ArrowUp");
  await page.keyboard.type(" world");
  await page.keyboard.press("Enter");
  await expect(terminal(page)).toContainText('"helloworld"');
  await page.keyboard.type("str first");
  await page.keyboard.press("Control+j");
  await page.keyboard.type(" second");
  await expect(terminal(page)).not.toContainText('"firstsecond"');
  await page.keyboard.press("Enter");
  await expect(terminal(page)).toContainText('"firstsecond"');
});

test("type inspects commands and supports capture and file redirection", async ({ page }) => {
  await playground(page);
  await execute(page, `(defn echo () (exit 9))
(type type echo cat)
(type -a echo)
(str (type -t cat))
(|> kinds.txt (type -t type cat))
(|>> kinds.txt (type -t echo))
(str (cat kinds.txt))
(str (|> explicit.txt (type -t type)))
(str (cat explicit.txt))
(if (|2> errors.txt (type no-such-parsh-command)) (exit 9))
(str (cat errors.txt))
(if (type -t missing-command) (exit 9) (str type-recovered))`);
  await expect(terminal(page)).toContainText("type is a shell builtin");
  await expect(terminal(page)).toContainText("echo is a function");
  await expect(terminal(page)).toContainText("echo is an external command");
  await expect(terminal(page)).toContainText("cat is an external command");
  await expect(terminal(page)).toContainText('"file"');
  await expect(terminal(page)).toContainText('"builtin\\nfile\\nfunction"');
  await expect(terminal(page)).toContainText('"builtin"');
  await expect(terminal(page)).toContainText('"type: no-such-parsh-command: not found"');
  await expect(terminal(page)).toContainText('"type-recovered"');
  await expect(terminal(page)).not.toContainText("exited with status");
});

test("bindings, closures, conditions, loops and Unicode retain shared semantics", async ({
  page,
}) => {
  await playground(page);
  await execute(
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
  await execute(page, "(export GREETING hello) (str $GREETING) (str name)");
  await expect(terminal(page)).toContainText('"hello"');
  await expect(terminal(page)).toContainText('"name"');
});

test("while and for share loop control, typed break values and fresh iteration scopes", async ({ page }) => {
  await playground(page);
  await execute(page, `(let count 0)
(let name outer)
(while (< $count 5)
  (set count (+ $count 1))
  (let name (str $name $count))
  (if (= $count 2) (continue))
  (if (= $count 4) (break (list stopped $count $name)))
  (str $name))
(list $count $name)
(for item (list first second third)
  (if (eq $item second) (continue))
  (if (eq $item third) (break (list for-stopped $item)))
  (str $item))
(while (false) (echo unavailable))`);
  await expect(terminal(page)).toContainText('("stopped" 4 "outer4")');
  await expect(terminal(page)).toContainText('(4 "outer")');
  await expect(terminal(page)).toContainText('("for-stopped" "third")');
  await execute(page, "(defn stop () (break)) (while (eq a a) (stop)) (str skipped)");
  await expect(terminal(page)).toContainText("in stop: break used outside a loop body");
  await expect(terminal(page)).not.toContainText('"skipped"');
  await execute(page, '(str recovered)');
  await expect(terminal(page)).toContainText('"recovered"');
});

test("loop keywords complete inside browser control flow", async ({ page }) => {
  await playground(page);
  await page.getByRole("textbox", { name: "parsh terminal input" }).focus();
  await page.keyboard.type("wh");
  await page.keyboard.press("Tab");
  await page.keyboard.type("(eq a a) (br");
  await page.keyboard.press("Tab");
  await page.keyboard.type("(list completed))");
  await expect(terminal(page)).toContainText("(while (eq a a) (break (list completed))");
  await page.keyboard.press("Enter");
  await expect(terminal(page)).toContainText('("completed")');
  await page.keyboard.type("for item (list a b) (cont");
  await page.keyboard.press("Tab");
  await expect(terminal(page)).toContainText("(for item (list a b) (continue");
  await page.keyboard.type(")");
  await page.keyboard.press("Enter");
  await execute(page, '(str completed-again)');
  await expect(terminal(page)).toContainText('"completed-again"');
});

test("quoted strings share variable and expression interpolation semantics", async ({ page }) => {
  await playground(page);
  await execute(page, String.raw`(let name parsh)
(let 名字 世界)
(defn name () (str called))
(list "hello, $name" "$(get name)_suffix" "sum=$(+ 1 2)"
  "$(name)" "nested=$(str "a" 'b')" "$名字")
(eq '$name $(+ 1 2)\n' "\$name \$(+ 1 2)\\n")
(str "capture=$(printf "line\n\n")")`);
  await expect(terminal(page)).toContainText(
    '("hello, parsh" "parsh_suffix" "sum=3" "called" "nested=ab" "世界")',
  );
  await expect.poll(async () =>
    (await terminal(page).innerText()).split("\n").some(line => line.trim() === "true"),
  ).toBe(true);
  await expect(terminal(page)).toContainText('"capture=line"');
  await execute(page, '(str "value=$missing") (str skipped-interpolation)');
  await expect(terminal(page)).toContainText("undefined variable $missing");
  await expect(terminal(page)).not.toContainText('"skipped-interpolation"');
  await execute(page, '(str "$(get name)_recovered")');
  await expect(terminal(page)).toContainText('"parsh_recovered"');
});

test("unfinished interpolations wait before executing the browser batch", async ({ page }) => {
  await playground(page);
  await paste(page, '(str BEFORE_INTERPOLATION) (str "sum=$(+ 1');
  await page.locator("[data-terminal-host] textarea").focus();
  await page.keyboard.press("Enter");
  await expect(terminal(page)).not.toContainText('"BEFORE_INTERPOLATION"');
  await page.keyboard.type(' 2)")');
  await page.keyboard.press("Enter");
  await expect(terminal(page)).toContainText('"BEFORE_INTERPOLATION"');
  await expect(terminal(page)).toContainText('"sum=3"');
});

test("triple quoted strings preserve newlines and distinguish interpolation from literal text", async ({ page }) => {
  await playground(page);
  await execute(page, String.raw`(let name parsh)
(list """""" '''''')
(str """hello, "$name"
sum=$(+ 1 2)""")
(str '''literal '$missing'
$(missing)\n''')
(str """nested=$(str """inner "$name" text""" ''''literal' '' quotes''')""")
(str """capture=$(printf "line\n\n")""")`);
  await expect(terminal(page)).toContainText('("" "")');
  await expect(terminal(page)).toContainText(String.raw`"hello, \"parsh\"\nsum=3"`);
  await expect(terminal(page)).toContainText(String.raw`"literal '\$missing'\n\$(missing)\\n"`);
  await expect(terminal(page)).toContainText(String.raw`"nested=inner \"parsh\" text'literal' '' quotes"`);
  await expect(terminal(page)).toContainText('"capture=line"');
});

test("triple quoted strings wait for all three closing quotes before executing a browser batch", async ({ page }) => {
  await playground(page);
  for (const [index, delimiter] of ['"""', "'''"].entries()) {
    const before = `BEFORE_TRIPLE_${index}`;
    await paste(page, `(str ${before}) (str ${delimiter}line`);
    await page.locator("[data-terminal-host] textarea").focus();
    await page.keyboard.press("Enter");
    await expect(terminal(page)).not.toContainText(`"${before}"`);
    await page.keyboard.type(`two${delimiter.slice(0, 2)}`);
    await page.keyboard.press("Enter");
    await expect(terminal(page)).not.toContainText(`"${before}"`);
    await page.keyboard.type(`end${delimiter})`);
    await page.keyboard.press("Enter");
    await expect(terminal(page)).toContainText(`"${before}"`);
    const text = `line\ntwo${delimiter.slice(0, 2)}\nend`;
    await expect(terminal(page)).toContainText(JSON.stringify(text));
  }
});

test("triple quoted completion preserves delimiters, variables, and raw dollar filenames", async ({ page }) => {
  await playground(page);
  await execute(page, "(let TRIPLE_NAME retained) (touch '$literal')");
  await page.locator("[data-terminal-host] textarea").focus();
  await page.keyboard.type('str """value=$TRIPLE_N');
  await page.keyboard.press("Tab");
  await expect(terminal(page)).toContainText('str """value=$TRIPLE_NAME');
  await page.keyboard.type('"""');
  await page.keyboard.press("Enter");
  await expect(terminal(page)).toContainText('"value=retained"');
  await page.keyboard.type('str """$(stri');
  await page.keyboard.press("Tab");
  await expect(terminal(page)).toContainText('str """$(strim ');
  await page.keyboard.type("'''inner''')!\"\"\"");
  await page.keyboard.press("Enter");
  await expect(terminal(page)).toContainText('"inner!"');
  await page.keyboard.type("str '''$l");
  await page.keyboard.press("Tab");
  await expect(terminal(page)).toContainText("str '''$literal'''");
  await page.keyboard.press("Enter");
  await expect(terminal(page)).toContainText(String.raw`"\$literal"`);
});

test("quoted completion handles interpolation and literal dollar filenames", async ({ page }) => {
  await playground(page);
  await execute(page, "(let TAB_NAME retained) (touch '$literal')");
  await page.locator("[data-terminal-host] textarea").focus();
  await page.keyboard.type('str "value=$TAB_N');
  await page.keyboard.press("Tab");
  await expect(terminal(page)).toContainText('str "value=$TAB_NAME');
  await page.keyboard.type('"');
  await page.keyboard.press("Enter");
  await expect(terminal(page)).toContainText('"value=retained"');
  await page.keyboard.type('str "$(stri');
  await page.keyboard.press("Tab");
  await expect(terminal(page)).toContainText('str "$(strim ');
  await page.keyboard.type('"inner")!"');
  await page.keyboard.press("Enter");
  await expect(terminal(page)).toContainText('"inner!"');
  await page.keyboard.type("str '$l");
  await page.keyboard.press("Tab");
  await expect(terminal(page)).toContainText("str '$literal'");
  await page.keyboard.press("Enter");
  await expect(terminal(page)).toContainText(String.raw`"\$literal"`);
});

test("binding targets reject variable references and allow recovery", async ({
  page,
}) => {
  await playground(page);
  await execute(page, "(let name retained) (set $name changed)");
  await expect(terminal(page)).toContainText(
    "binding must use a bare variable name without '$'",
  );
  await execute(page, "(str $name)");
  await expect(terminal(page)).toContainText('"retained"');
});

test("numeric comparisons preserve precision and work in browser conditions", async ({ page }) => {
  await playground(page);
  await execute(page, `(list (= 001 1.0) (!= 1 1.5) (< 2 10)
    (<= 3 3.0) (> 10 2) (>= 2.5 3))
(list (> 9007199254740993 9007199254740992.0)
    (< 9223372036854775807 9223372036854775808.0) (>= -0.0 0))
(let count (printf " 3\\n"))
(if (>= $count 3.0) (str enough) (str missing))`);
  await expect(terminal(page)).toContainText("(true true true true true false)");
  await expect(terminal(page)).toContainText("(true true true)");
  await expect(terminal(page)).toContainText('"enough"');
  await execute(page, "(> 1 invalid) (str skipped)");
  await expect(terminal(page)).toContainText("invalid number");
  await expect(terminal(page)).not.toContainText('"skipped"');
  await execute(page, "(if (< 1 2) (str recovered))");
  await expect(terminal(page)).toContainText('"recovered"');
});

test("paste waits for Enter and syntax errors preserve the entire draft", async ({ page }) => {
  await playground(page);
  await paste(page, "(str first) (str second)");
  await expect(terminal(page)).toContainText("(str second)");
  await expect(terminal(page)).not.toContainText('"first"');
  await page.locator("[data-terminal-host] textarea").focus();
  await page.keyboard.press("Enter");
  await expect(terminal(page)).toContainText('"second"');
  await execute(page, "(str never-executed))) (str skipped)");
  await expect(terminal(page)).toContainText("unexpected ')'");
  await expect(terminal(page)).not.toContainText('"never-executed"');
  await page.keyboard.press("Control+_");
  await execute(page, "(no-such-command) (str skipped)");
  await expect(terminal(page)).toContainText("command 'no-such-command' is not available in this playground");
  await expect(terminal(page)).not.toContainText('"skipped"');
  await execute(page, "(str recovered)");
  await expect(terminal(page)).toContainText('"recovered"');
});

test("unavailable commands produce source-tagged diagnostics and recover", async ({ page }) => {
  await playground(page);
  await execute(page, "(no-such-command)");
  await expect(terminal(page)).toContainText("interactive:1:1:");
  await expect(terminal(page)).toContainText("is not available in this playground");
  await page.getByRole("textbox", { name: "parsh terminal input" }).focus();
  await page.keyboard.press("Control+d");
  await expect(terminal(page)).toContainText("[exit 127]");
  await page.getByRole("button", { name: "Reset", exact: true }).click();
  await expect(page.locator("[data-playground]")).toHaveAttribute("data-state", "ready");
  await execute(page, "(echo recovered)");
  await expect(terminal(page)).toContainText("recovered");
});

test("clear and idle Ctrl+C preserve bindings; reset clears them", async ({
  page,
}) => {
  await playground(page);
  await execute(page, "(let name retained) (str $name)");
  await expect(terminal(page)).toContainText('"retained"');
  await page.getByRole("button", { name: "Clear", exact: true }).click();
  await expect(terminal(page)).not.toContainText('"retained"');
  const input = page.getByRole("textbox", { name: "parsh terminal input" });
  await input.focus();
  await page.keyboard.type("str unfinished");
  await page.keyboard.press("Control+c");
  await execute(page, "(str $name)");
  await expect(terminal(page)).toContainText('"retained"');
  await page.getByRole("button", { name: "Reset", exact: true }).click();
  await expect(page.locator("[data-playground]")).toHaveAttribute(
    "data-state",
    "ready",
  );
  await execute(page, "(str $name)");
  await expect(terminal(page)).toContainText("undefined variable $name");
});

test("running Ctrl+C stops an infinite while loop and resets the session", async ({
  page,
}) => {
  await playground(page);
  await page.getByRole("textbox", { name: "parsh terminal input" }).focus();
  await execute(page, "(let name before) (while (eq a a) (continue))");
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
  await execute(page, "(str $name)");
  await expect(terminal(page)).toContainText("undefined variable $name");
});

test("an infinite while loop times out without freezing the page", async ({
  page,
}) => {
  test.setTimeout(20_000);
  await playground(page);
  await execute(page, "(while (eq a a) (do))");
  await expect(page.locator("[data-notice]")).toContainText(
    "Execution exceeded 5 seconds",
    { timeout: 12_000 },
  );
  await expect(page.locator("[data-playground]")).toHaveAttribute(
    "data-state",
    "ready",
  );
  await execute(page, "(str responsive)");
  await expect(terminal(page)).toContainText('"responsive"');
});

test("exit and Ctrl+D end a session until reset", async ({ page }) => {
  await playground(page);
  await execute(page, "(exit 7)");
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
  await execute(page, "(str retried)");
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

test("structs and macros evaluate in the playground", async ({ page }) => {
  await playground(page);
  await execute(page, "(defstruct Point x y)");
  await execute(page, "(let point (Point 3 4))");
  await execute(page, "(Point.x $point)");
  await expect(terminal(page)).toContainText('"3"');
  await execute(
    page,
    "(defmacro unless (condition body) `(if (not ,$condition) ,$body))",
  );
  await execute(page, "(unless (eq 1 2) (str taken))");
  await expect(terminal(page)).toContainText('"taken"');
  await execute(page, "(type Point.x)");
  await expect(terminal(page)).toContainText("Point.x is a struct accessor");
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
  await execute(page, String.raw`(echo "你好，parsh 🐑") (export GREETING "你好 🐑") (printenv GREETING)`);
  await expect(terminal(page)).toContainText("你好，parsh 🐑");
  await expect(terminal(page)).toContainText("你好 🐑");
  await execute(page, String.raw`(defn capture () (printf "%s\n" "a value with spaces"))
(let value (capture))
(printf "<%s>\\n" $value)`);
  await expect(terminal(page)).toContainText("<a value with spaces>");
  await execute(page, String.raw`(printf "no-newline") (str complete) (| (printf "\\377\\000\\101") (base64))`);
  await expect(terminal(page)).toContainText("no-newline");
  await expect(terminal(page)).toContainText('"complete"');
  await expect(terminal(page)).toContainText("/wBB");
  await execute(page, String.raw`(printf "%0400d\n" 7)`);
  await expect(terminal(page)).toContainText("000007");
});

test("file commands, glob, cwd, and all redirection forms share temporary files", async ({ page }) => {
  await playground(page);
  await execute(page, String.raw`(mkdir -p demo) (cd demo)
(|> first.txt (printf "one\ntwo\n"))
(|1>> first.txt (echo three))
(cp first.txt copy.txt) (mv copy.txt moved.txt)
(touch empty.txt .hidden.txt) (str prepared)`);
  await expect(terminal(page)).toContainText('"prepared"');
  await execute(page, String.raw`(printf "glob:<%s>\\n" (glob *.txt))`);
  await expect(terminal(page)).toContainText("glob:<first.txt>");
  await expect(terminal(page)).toContainText("glob:<moved.txt>");
  await expect(terminal(page)).not.toContainText("glob:<.hidden.txt>");
  await execute(page, String.raw`(cat /home/playground/demo/first.txt) (cat ../numbers.txt)`);
  await expect(terminal(page)).toContainText("three");
  await expect(terminal(page)).toContainText("30");
  await execute(page, String.raw`(<| first.txt (sort -r))`);
  await expect(terminal(page)).toContainText("two");
  await execute(page, String.raw`(|2> errors.txt (if (cat missing.txt) (echo unexpected) (echo expected-failure)))`);
  await expect(terminal(page)).toContainText("expected-failure");
  await execute(page, String.raw`(|2>> errors.txt (if (cat another-missing.txt) (echo unexpected) (echo expected-again))) (cat errors.txt)`);
  await expect(terminal(page)).toContainText("expected-again");
  await expect(terminal(page)).toContainText("another-missing.txt: No such file");
  await execute(page, String.raw`(export PWD /bogus) (str (pwd)) (|> nested.txt (echo (pwd))) (cat nested.txt)`);
  await expect(terminal(page)).toContainText('"/home/playground/demo"');
  await execute(page, String.raw`(cd ..) (rm demo/empty.txt) (mkdir disposable) (rmdir disposable) (str (pwd))`);
  await expect(terminal(page)).toContainText('"/home/playground"');
});

test("pipelines preserve statuses and redirection file positions across commands", async ({ page }) => {
  await playground(page);
  await execute(page, String.raw`(| (sort fruits.txt) (uniq -c))`);
  await expect(terminal(page)).toContainText("2 apple");
  await execute(page, String.raw`(if (| (false) (true)) (echo wrong-branch) (echo right-branch))
(|1> out.txt (do (printf one) (printf two)))
(|>> out.txt (printf three)) (echo (cat out.txt))`);
  await expect(terminal(page)).toContainText("right-branch");
  await expect(terminal(page)).toContainText("onetwothree");
  await execute(page, String.raw`(|> binary.txt (printf "\\377\\000\\101")) (<| binary.txt (base64))`);
  await expect(terminal(page)).toContainText("/wBB");
  await execute(page, String.raw`(| (str invalid) (cat))`);
  await expect(terminal(page)).toContainText("v1 pipeline stages must be external commands");
  await execute(page, String.raw`(| (false) (true)) (str skipped-after-failure)`);
  await expect(terminal(page)).toContainText("false | true exited with status 1");
  await expect(terminal(page)).not.toContainText('"skipped-after-failure"');
});

test("clear preserves files while reset recreates the initial filesystem", async ({ page }) => {
  await playground(page);
  await execute(page, "(|> retained.txt (echo retained-file))");
  await page.getByRole("button", { name: "Clear", exact: true }).click();
  await execute(page, "(cat retained.txt)");
  await expect(terminal(page)).toContainText("retained-file");
  await page.getByRole("button", { name: "Reset", exact: true }).click();
  await expect(page.locator("[data-playground]")).toHaveAttribute("data-state", "ready");
  await execute(page, "(if (test -f retained.txt) (echo stale-file) (echo fresh-files)) (cat fruits.txt)");
  await expect(terminal(page)).toContainText("fresh-files");
  await expect(terminal(page)).toContainText("banana");
});

test("coreutils output and filesystem quotas fail without losing the session", async ({ page }) => {
  test.setTimeout(30_000);
  await playground(page);
  await execute(page, "(let retained yes) (seq 1 2000000)");
  await expect(terminal(page)).toContainText("exceeds 8 MiB");
  await execute(page, "(str $retained)");
  await expect(terminal(page)).toContainText('"yes"');
  await execute(page, '(|> chunk.txt (seq 1 600000)) (str chunk-ready)');
  await expect(terminal(page)).toContainText('"chunk-ready"');
  await execute(page, Array.from({ length: 8 }, (_, index) => `(cp chunk.txt copy${index}.txt)`).join(" "));
  await expect(terminal(page)).toContainText("virtual files exceed 32 MiB", { timeout: 15_000 });
  await execute(page, "(echo after-limit)");
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
    await execute(page, "(echo retried-command)");
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
  await page.keyboard.type("BROWSER_TAB_COMMAND");
  await page.keyboard.press("Enter");
  await expect.poll(async () =>
    (await terminal(page).innerText()).split("\n").some(line => line.trim() === "BROWSER_TAB_COMMAND"),
  ).toBe(true);
  await page.keyboard.type("cat fru");
  await page.keyboard.press("Tab");
  await expect(terminal(page)).toContainText("(cat fruits.txt");
  await expect(terminal(page)).not.toContainText("banana");
  await page.keyboard.press("Enter");
  await expect(terminal(page)).toContainText("banana");
  await execute(page, "(let TAB_VALUE retained)");
  await page.keyboard.type("str $TAB_V");
  await page.keyboard.press("Tab");
  await expect(terminal(page)).toContainText("(str $TAB_VALUE");
  await page.keyboard.press("Enter");
  await expect(terminal(page)).toContainText('"retained"');
});

test("double Tab lists candidates and preserves the prompt and further editing", async ({ page }) => {
  await playground(page);
  await execute(page, "(defn tab-alpha () (str TAB_ALPHA)) (defn tab-alpine () (str TAB_ALPINE))");
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
  await page.keyboard.press("Enter");
  await expect(terminal(page)).toContainText('"TAB_ALPHA"');
  await page.keyboard.type("(; ec");
  await page.keyboard.press("Tab");
  await page.keyboard.press("Enter");
  await expect(terminal(page)).toContainText("(; ec)");
});

test("Tab quotes Unicode paths and follows directories in the narrow Chinese terminal", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await playground(page, "zh-cn");
  await execute(page, '(mkdir "folder name") (touch "folder name/你好 file.txt")');
  await page.getByRole("textbox", { name: "parsh 终端输入" }).focus();
  await page.keyboard.type("str fol");
  await page.keyboard.press("Tab");
  await expect(terminal(page)).toContainText('"folder name/');
  await page.keyboard.press("Tab");
  await page.keyboard.press("Enter");
  await expect(terminal(page)).toContainText('"folder name/你好 file.txt"');
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
});

test("Emacs editing, kill ring, escape prefixes, and undo work in the browser", async ({ page }) => {
  await playground(page);
  const input = page.getByRole("textbox", { name: "parsh terminal input" });
  await input.focus();
  const cases: { text: string; actions: string[]; expected: string }[] = [
    { text: "str ac", actions: ["Control+b", "b", "Control+f", "Enter"], expected: '"abc"' },
    { text: "str XY", actions: ["Control+h", "Z", "Enter"], expected: '"XZ"' },
    { text: "str path/file.txt", actions: ["Alt+b", "Alt+d", "X", "Control+e", "Enter"], expected: '"path/file.X"' },
    { text: "str one two", actions: ["Control+w", "Alt+Backspace", "Control+y", "Enter"], expected: '"onetwo"' },
    { text: "str first second", actions: ["Control+w", "Control+b", "Control+w", "Control+y", "Alt+y", "Control+e", "Enter"], expected: '"second"' },
    { text: "str RAW", actions: ["Control+b", "X", "Control+_", "Control+e", "Enter"], expected: '"RAW"' },
    { text: "str CHORD", actions: ["Control+b", "X", "Control+x", "Control+u", "Control+e", "Enter"], expected: '"CHORD"' },
    { text: "str path/file.txt", actions: ["Escape", "b", "X", "Control+e", "Enter"], expected: '"path/file.Xtxt"' },
    { text: "str 甲e\u{301}👩‍💻乙", actions: ["Control+t", "Enter"], expected: '"甲e\\u{301}乙👩\\u{200d}💻"' },
  ];
  for (const { text, actions, expected } of cases) {
    // Seed Unicode through the existing paste path; xterm's screen-reader mode
    // does not forward synthetic insertText events.
    if (/[^\x00-\x7f]/.test(text)) await paste(page, text);
    else await page.keyboard.type(text);
    for (const action of actions) await page.keyboard.press(action);
    await expect(terminal(page)).toContainText(expected);
  }
});

test("Emacs search accepts without execution and abort restores the draft cursor", async ({ page }) => {
  await playground(page);
  await execute(page, "(str SEARCH_FIRST)(str SEARCH_SECOND)");
  await expect(terminal(page)).toContainText('"SEARCH_SECOND"');
  await page.getByRole("textbox", { name: "parsh terminal input" }).focus();
  await page.keyboard.type("str saved");
  await page.keyboard.press("Control+b");
  await page.keyboard.press("Control+r");
  await page.keyboard.type("SEARCH");
  await expect(terminal(page)).toContainText("(reverse-i-search)`SEARCH':");
  await page.keyboard.press("Control+r");
  await page.keyboard.press("Control+s");
  await expect(terminal(page)).toContainText("(forward-i-search)`SEARCH':");
  await page.keyboard.press("Control+g");
  await page.keyboard.type("X");
  await page.keyboard.press("Control+e");
  await page.keyboard.press("Enter");
  await expect(terminal(page)).toContainText('"saveXd"');

  await page.keyboard.press("Control+r");
  await page.keyboard.type("SEARCH_SECOND");
  await page.keyboard.press("Enter");
  await expect.poll(async () => (await terminal(page).innerText()).split('"SEARCH_SECOND"').length - 1).toBe(1);
  await page.keyboard.type("_ACCEPTED");
  await page.keyboard.press("Enter");
  await expect(terminal(page)).toContainText('"SEARCH_SECOND_ACCEPTED"');
});

test("Emacs search treats pasted source as a query and survives clear screen", async ({ page }) => {
  await playground(page);
  await execute(page, "(let retained value)(str TARGET)");
  await expect(terminal(page)).toContainText('"TARGET"');
  await page.getByRole("textbox", { name: "parsh terminal input" }).focus();
  await page.keyboard.type("str $retained");
  await page.keyboard.press("Control+r");
  await paste(page, "(exit 9)");
  await expect(terminal(page)).toContainText("(failed reverse-i-search)`(exit 9)':");
  await page.keyboard.press("Control+l");
  await expect(terminal(page)).toContainText("(failed reverse-i-search)`(exit 9)':");
  await page.keyboard.press("Escape");
  await page.keyboard.press("Enter");
  await expect(terminal(page)).toContainText('"value"');
  await expect(page.locator("[data-playground]")).toHaveAttribute("data-state", "ready");
});

test("Emacs browser shortcuts are scoped to terminal focus", async ({ page }) => {
  await playground(page);
  await page.getByRole("textbox", { name: "parsh terminal input" }).focus();
  await page.keyboard.type("str draft");
  await page.locator("[data-clear]").focus();
  await page.keyboard.press("Control+b");
  await expect(terminal(page)).not.toContainText("i-search");
  await page.getByRole("textbox", { name: "parsh terminal input" }).focus();
  await page.keyboard.press("Enter");
  await expect(terminal(page)).toContainText('"draft"');
});
