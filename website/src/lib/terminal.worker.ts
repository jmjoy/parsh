import type {
  WorkerRequest,
  WorkerResponse,
  TerminalEvent,
} from "./terminal-protocol";

interface WasmSession {
  input(key: string, text: string): TerminalEvent[];
  paste(source: string): TerminalEvent[];
  resize(columns: number, rows: number): TerminalEvent[];
  reset(): TerminalEvent[];
}
interface WasmModule {
  default(options: { module_or_path: string }): Promise<unknown>;
  Session: new (onActivity: (busy: boolean) => void) => WasmSession;
}

let session: WasmSession | undefined;
const send = (message: WorkerResponse) => self.postMessage(message);
self.onmessage = async ({ data }: MessageEvent<WorkerRequest>) => {
  try {
    if (data.type === "init") {
      const moduleUrl = `${data.base}wasm/parsh.js`;
      const wasm = (await import(/* @vite-ignore */ moduleUrl)) as WasmModule;
      await wasm.default({ module_or_path: `${data.base}wasm/parsh_bg.wasm` });
      session = new wasm.Session((busy) => send({ type: "activity", busy }));
      send({ type: "ready" });
      send({ type: "events", events: session.resize(data.columns, data.rows) });
      return;
    }
    if (!session) throw new Error("The terminal is still loading.");
    let events: TerminalEvent[];
    switch (data.type) {
      case "input":
        events = session.input(data.key, data.text ?? "");
        break;
      case "paste":
        events = session.paste(data.source);
        break;
      case "resize":
        events = session.resize(data.columns, data.rows);
        break;
      case "reset":
        events = session.reset();
        break;
    }
    send({ type: "events", events });
  } catch (error) {
    send({
      type: "error",
      message: error instanceof Error ? error.message : String(error),
    });
  }
};
