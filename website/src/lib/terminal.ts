import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import "@xterm/xterm/css/xterm.css";
import { base } from "./site";
import type {
  TerminalEvent,
  WorkerRequest,
  WorkerResponse,
} from "./terminal-protocol";

type Phase = "loading" | "ready" | "running" | "closed" | "error";
const examples: Record<string, string> = {
  hello: '(str "hello, " "parsh")\n(list text "a value with spaces" (eq a a))',
  variables:
    '(let name world)\n(str "hello, " $name)\n(set name parsh)\n(str "hello, " $name)',
  functions:
    '(defn greet (who)\n  (str "hello, " $who))\n(greet parsh)\n(greet "a friend")',
  control:
    '(let items (list first second last))\n(if (not (eq first last))\n    (str "different words")\n    (str "same word"))\n(for item $items\n  (str "last result: " $item))',
};
const keys: Record<string, string> = {
  "\r": "enter",
  "\n": "enter",
  "\t": "tab",
  "\x7f": "backspace",
  "\b": "backspace",
  "\x1b[A": "up",
  "\x1b[B": "down",
  "\x1b[C": "right",
  "\x1b[D": "left",
  "\x1bOA": "up",
  "\x1bOB": "down",
  "\x1bOC": "right",
  "\x1bOD": "left",
  "\x1b[H": "home",
  "\x1b[F": "end",
  "\x1bOH": "home",
  "\x1bOF": "end",
  "\x1b[1~": "home",
  "\x1b[4~": "end",
  "\x1b[3~": "delete",
  "\x01": "home",
  "\x05": "end",
  "\x10": "previous",
  "\x0e": "next",
  "\x15": "kill-start",
  "\x0b": "kill-end",
  "\x04": "eof",
};

export function mountTerminal(root: HTMLElement): void {
  const zh = root.dataset.locale === "zh-cn";
  const labels = zh
    ? {
        loading: "正在加载解释器…",
        ready: "可以开始输入",
        running: "正在求值…",
        closed: "会话已结束",
        error: "加载失败",
        input: "parsh 终端输入",
        welcome: "parsh 语法体验终端 · 输入 str hello) 开始。",
        interrupted: "执行已中断，会话已重置。",
        timeout: "执行超过 5 秒，已停止并重置会话。",
        failed: "无法加载解释器，请检查网络后重试。",
        ended: "会话已结束。点击“重置”开始新的会话。",
        reset: "已开始新的会话。",
      }
    : {
        loading: "Loading the interpreter…",
        ready: "Ready when you are",
        running: "Evaluating…",
        closed: "Session ended",
        error: "Could not load",
        input: "parsh terminal input",
        welcome: "parsh syntax playground · Type str hello) to begin.",
        interrupted: "Execution interrupted. A new session has started.",
        timeout: "Execution exceeded 5 seconds. The session has been reset.",
        failed:
          "The interpreter could not load. Check your connection and retry.",
        ended: "This session has ended. Choose Reset to start a new one.",
        reset: "A new session has started.",
      };
  const find = <T extends HTMLElement>(selector: string): T => {
    const element = root.querySelector<T>(selector);
    if (!element) throw new Error(`Missing terminal element: ${selector}`);
    return element;
  };
  const host = find<HTMLElement>("[data-terminal-host]");
  const status = find<HTMLElement>("[data-status]");
  const state = find<HTMLElement>(".terminal-state");
  const notice = find<HTMLElement>("[data-notice]");
  const noticeText = find<HTMLElement>("[data-notice-text]");
  const retry = find<HTMLButtonElement>("[data-retry]");
  const controls = root.querySelectorAll<HTMLButtonElement>(
    "[data-example], [data-clear], [data-interrupt]",
  );
  const terminal = new Terminal({
    fontFamily: '"SFMono-Regular", Consolas, "Liberation Mono", monospace',
    fontSize: matchMedia("(max-width: 480px)").matches ? 12 : 14,
    lineHeight: 1.45,
    cursorBlink: true,
    cursorStyle: "bar",
    scrollback: 2000,
    screenReaderMode: true,
    allowProposedApi: false,
    theme: {
      background: "#1b241c",
      foreground: "#e4e9d7",
      cursor: "#d9e6bd",
      selectionBackground: "#526447",
      black: "#1b241c",
      brightBlack: "#a4b095",
      red: "#ff9c80",
      green: "#c0d898",
      yellow: "#e7c69a",
      blue: "#bacbc4",
      magenta: "#d0c0c8",
      cyan: "#bbd1b3",
      white: "#e4e9d7",
    },
  });
  const fit = new FitAddon();
  terminal.loadAddon(fit);
  terminal.open(host);
  fit.fit();
  const textarea = terminal.textarea;
  if (textarea) textarea.setAttribute("aria-label", labels.input);

  let worker: Worker | undefined;
  let phase: Phase = "loading";
  let generation = 0;
  let timer: ReturnType<typeof setTimeout> | undefined;
  let drawnRows = 0;
  let cursorRow = 0;
  let disposed = false;

  function setPhase(next: Phase): void {
    phase = next;
    root.dataset.state = next;
    state.dataset.state = next;
    status.textContent = labels[next];
    controls.forEach((control) => {
      control.disabled = next !== "ready" && next !== "running";
    });
  }
  function showNotice(text: string, canRetry = false): void {
    noticeText.textContent = text;
    notice.hidden = false;
    retry.hidden = !canRetry;
  }
  function send(message: WorkerRequest): void {
    worker?.postMessage(message);
  }
  function clearTimer(): void {
    if (timer) clearTimeout(timer);
    timer = undefined;
  }
  function wipeDisplay(): void {
    terminal.reset();
    terminal.write("\x1b[?7l"); // The shared Rust layout handles wrapping.
    drawnRows = 0;
    cursorRow = 0;
  }
  function render(event: Extract<TerminalEvent, { type: "render" }>): void {
    let output = "\r";
    if (cursorRow > 0) output += `\x1b[${cursorRow}A`;
    output += "\x1b[J";
    output += event.lines
      .map((line, index) =>
        event.dim_prompt && index === 0
          ? `\x1b[90m(\x1b[0m${line.slice(1)}`
          : line,
      )
      .join("\r\n");
    const up = event.lines.length - 1 - event.row;
    if (up > 0) output += `\x1b[${up}A`;
    output += `\x1b[${event.column + 1}G`;
    terminal.write(output);
    drawnRows = event.lines.length;
    cursorRow = event.row;
  }
  function commit(): void {
    const down = drawnRows - 1 - cursorRow;
    terminal.write(`${down > 0 ? `\x1b[${down}B` : ""}\r\n`);
    drawnRows = 0;
    cursorRow = 0;
  }
  function apply(events: TerminalEvent[]): void {
    for (const event of events) {
      switch (event.type) {
        case "render":
          render(event);
          break;
        case "commit":
          commit();
          break;
        case "value":
          terminal.write(`\x1b[32m${event.text}\x1b[0m\r\n`);
          break;
        case "diagnostic":
          terminal.write(`\x1b[31m${event.text}\x1b[0m\r\n`);
          break;
        case "exit":
          clearTimer();
          setPhase("closed");
          terminal.write(`\x1b[90m[exit ${event.code}]\x1b[0m\r\n`);
          showNotice(labels.ended);
          break;
      }
    }
  }
  function fail(error: unknown): void {
    clearTimer();
    worker?.terminate();
    setPhase("error");
    showNotice(labels.failed, true);
    console.error("parsh interpreter:", error);
  }
  function start(reason?: string): void {
    worker?.terminate();
    clearTimer();
    const currentGeneration = ++generation;
    setPhase("loading");
    notice.hidden = true;
    wipeDisplay();
    terminal.write(`\x1b[90m${labels.welcome}\x1b[0m\r\n\r\n`);
    if (reason) showNotice(reason);
    try {
      worker = new Worker(new URL("./terminal.worker.ts", import.meta.url), {
        type: "module",
      });
      worker.onmessage = ({ data }: MessageEvent<WorkerResponse>) => {
        if (currentGeneration !== generation || disposed) return;
        switch (data.type) {
          case "ready":
            setPhase("ready");
            break;
          case "activity":
            clearTimer();
            if (data.busy) {
              setPhase("running");
              timer = setTimeout(() => start(labels.timeout), 5000);
            } else if (phase !== "closed") setPhase("ready");
            break;
          case "events":
            apply(data.events);
            break;
          case "error":
            fail(data.message);
            break;
        }
      };
      worker.onerror = (event) => {
        if (currentGeneration === generation) {
          event.preventDefault();
          fail(event.message);
        }
      };
      worker.onmessageerror = () =>
        fail("Unable to read the interpreter response.");
      const originBase = new URL(base, location.origin).href;
      send({
        type: "init",
        base: originBase,
        columns: terminal.cols,
        rows: terminal.rows,
      });
    } catch (error) {
      fail(error);
    }
  }
  function interrupt(): void {
    if (phase === "running") start(labels.interrupted);
    else if (phase === "ready") send({ type: "input", key: "interrupt" });
    terminal.focus();
  }
  function clear(): void {
    if (phase !== "ready") return;
    wipeDisplay();
    send({ type: "resize", columns: terminal.cols, rows: terminal.rows });
    terminal.focus();
  }
  terminal.attachCustomKeyEventHandler((event) => {
    if (event.type !== "keydown") return true;
    if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "c") {
      if (terminal.hasSelection() || event.metaKey || event.shiftKey)
        return false;
      event.preventDefault();
      interrupt();
      return false;
    }
    if (event.ctrlKey && event.key.toLowerCase() === "l") {
      event.preventDefault();
      clear();
      return false;
    }
    return true;
  });
  terminal.onData((data) => {
    if (phase !== "ready" && phase !== "running") return;
    if (data === "\x03") {
      interrupt();
      return;
    }
    const key = keys[data];
    if (key) send({ type: "input", key });
    else if (!data.startsWith("\x1b") && !/[\x00-\x1f\x7f]/.test(data))
      send({ type: "input", key: "text", text: data });
  });
  host.addEventListener(
    "paste",
    (event) => {
      if (phase !== "ready" || !event.clipboardData) return;
      event.preventDefault();
      event.stopImmediatePropagation();
      send({
        type: "paste",
        source: event.clipboardData.getData("text/plain"),
      });
    },
    true,
  );
  find<HTMLButtonElement>("[data-clear]").addEventListener("click", clear);
  find<HTMLButtonElement>("[data-interrupt]").addEventListener(
    "click",
    interrupt,
  );
  find<HTMLButtonElement>("[data-reset]").addEventListener("click", () => {
    start(labels.reset);
    terminal.focus();
  });
  retry.addEventListener("click", () => {
    start();
    terminal.focus();
  });
  root.querySelectorAll<HTMLButtonElement>("[data-example]").forEach((button) =>
    button.addEventListener("click", () => {
      if (phase !== "ready") return;
      const source = examples[button.dataset.example ?? ""];
      if (!source) return;
      notice.hidden = true;
      send({ type: "input", key: "clear-input" });
      send({ type: "paste", source });
      terminal.focus();
    }),
  );
  const resize = new ResizeObserver(() => {
    if (disposed) return;
    terminal.options.fontSize = matchMedia("(max-width: 480px)").matches
      ? 12
      : 14;
    fit.fit();
    if (phase === "ready" || phase === "running")
      send({ type: "resize", columns: terminal.cols, rows: terminal.rows });
  });
  resize.observe(host);
  window.addEventListener(
    "pagehide",
    (event) => {
      if (event.persisted) return;
      disposed = true;
      clearTimer();
      worker?.terminate();
      resize.disconnect();
      terminal.dispose();
    },
    { once: true },
  );
  start();
}
