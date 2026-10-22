export type TerminalEvent =
  | {
      type: "render";
      lines: string[];
      row: number;
      column: number;
      dim_prompt: boolean;
    }
  | { type: "output"; stream: "stdout" | "stderr"; bytes: number[] }
  | { type: "commit" }
  | { type: "completions"; lines: string[] }
  | { type: "value" | "diagnostic"; text: string }
  | { type: "exit"; code: number };

export type WorkerRequest =
  | { type: "init"; base: string; columns: number; rows: number }
  | { type: "input"; key: string; text?: string }
  | { type: "paste"; source: string }
  | { type: "resize"; columns: number; rows: number }
  | { type: "reset" };

export type WorkerResponse =
  | { type: "ready" }
  | { type: "activity"; busy: boolean }
  | { type: "events"; events: TerminalEvent[] }
  | { type: "error"; message: string };
