import {
  WASI, File, Directory, PreopenDirectory, OpenFile, ConsoleStdout, wasi as defs,
} from "@bjorn3/browser_wasi_shim";

const OUTPUT_LIMIT = 8 * 1024 * 1024;
const FILE_LIMIT = 32 * 1024 * 1024;
const HOME = "/home/playground";
const encoder = new TextEncoder();

interface RunRequest {
  stages: string[][];
  cwd: string;
  environment: Map<string, string>;
  input?: number;
  output?: number;
  error?: number;
  capture: boolean;
}
interface Chunk { stream: "stdout" | "stderr"; bytes: Uint8Array }
class HostFailure extends Error {
  constructor(message: string, readonly code = 2) { super(message); }
}

// 0.4.0 uses incorrect offsets/padding for the WASI Preview 1 filestat ABI.
defs.Filestat.prototype.write_bytes = function(view: DataView, ptr: number) {
  view.setBigUint64(ptr, this.dev, true);
  view.setBigUint64(ptr + 8, this.ino, true);
  view.setBigUint64(ptr + 16, BigInt(this.filetype), true);
  view.setBigUint64(ptr + 24, this.nlink, true);
  view.setBigUint64(ptr + 32, this.size, true);
  view.setBigUint64(ptr + 40, this.atim, true);
  view.setBigUint64(ptr + 48, this.mtim, true);
  view.setBigUint64(ptr + 56, this.ctim, true);
};

export class CoreutilsHost {
  private root = new Directory(new Map());
  private handles = new Map<number, OpenFile>();
  private nextHandle = 1;
  private readonly commands: Set<string>;

  constructor(private readonly module: WebAssembly.Module, commands: string[]) {
    this.commands = new Set(commands);
  }

  static async load(base: string): Promise<CoreutilsHost> {
    const [response, manifest] = await Promise.all([
      fetch(`${base}wasm/coreutils.wasm`),
      fetch(`${base}wasm/coreutils.json`),
    ]);
    if (!response.ok || !manifest.ok) throw new Error("Could not load coreutils.");
    const module = await WebAssembly.compile(await response.arrayBuffer());
    const { commands } = await manifest.json() as { commands: string[] };
    return new CoreutilsHost(module, commands);
  }

  readonly call = (operation: string, args: unknown): unknown => {
    try {
      let value: unknown;
      switch (operation) {
        case "reset": value = this.reset(); break;
        case "run": value = this.run(args as RunRequest); break;
        case "open": {
          const [cwd, path, mode] = args as [string, string, string];
          value = this.open(cwd, path, mode); break;
        }
        case "close": this.handles.delete(args as number); value = true; break;
        case "chdir": {
          const [cwd, path] = args as [string, string];
          const absolute = this.resolve(cwd, path);
          if (!(this.lookup(absolute) instanceof Directory)) throw new HostFailure(`cd: ${path}: not a directory`);
          value = absolute; break;
        }
        case "paths": value = this.paths(); break;
        default: throw new HostFailure(`unknown host operation: ${operation}`);
      }
      return { value };
    } catch (error) {
      return { error: error instanceof Error ? error.message : String(error), code: error instanceof HostFailure ? error.code : 2 };
    }
  };

  private reset(): string {
    this.handles.clear();
    this.nextHandle = 1;
    this.root = new Directory(new Map([
      ["home", new Directory(new Map([["playground", new Directory(new Map([
        ["fruits.txt", new File(encoder.encode("banana\napple\nbanana\ncherry\napple\n"))],
        ["numbers.txt", new File(encoder.encode("30\n2\n10\n1\n"))],
        ["README.txt", new File(encoder.encode("Welcome to parsh!\nFiles live only in this playground session.\nTry (| (sort fruits.txt) (uniq -c)).\n"))],
      ]))]]))],
      ["tmp", new Directory(new Map())],
    ]));
    return HOME;
  }

  private resolve(cwd: string, path: string): string {
    if (!path || path.includes("\0")) throw new HostFailure("invalid empty path or NUL in path");
    const parts: string[] = [];
    for (const part of (path.startsWith("/") ? path : `${cwd}/${path}`).split("/")) {
      if (part === "..") parts.pop();
      else if (part && part !== ".") parts.push(part);
    }
    return `/${parts.join("/")}`;
  }

  private lookup(path: string): File | Directory {
    let node: File | Directory = this.root;
    for (const part of path.split("/").filter(Boolean)) {
      if (!(node instanceof Directory)) throw new HostFailure(`${path}: not a directory`);
      const child = node.contents.get(part);
      if (!(child instanceof File || child instanceof Directory)) throw new HostFailure(`${path}: no such file or directory`);
      node = child;
    }
    return node;
  }

  private open(cwd: string, path: string, mode: string): number {
    if (path.endsWith("/")) throw new HostFailure(`${path}: not a file`);
    const absolute = this.resolve(cwd, path);
    const parentPath = absolute.slice(0, absolute.lastIndexOf("/")) || "/";
    const name = absolute.slice(absolute.lastIndexOf("/") + 1);
    const parent = this.lookup(parentPath);
    if (!(parent instanceof Directory) || !name) throw new HostFailure(`${path}: not a file`);
    let file = parent.contents.get(name);
    if (file === undefined && mode !== "read") {
      file = new File(new Uint8Array());
      parent.contents.set(name, file);
    }
    if (!(file instanceof File)) throw new HostFailure(`${path}: ${file ? "is a directory" : "no such file"}`);
    if (mode === "write") file.data = new Uint8Array();
    const fd = new OpenFile(file);
    if (mode === "append") fd.file_pos = file.size;
    const id = this.nextHandle++;
    this.handles.set(id, fd);
    return id;
  }

  private paths(): string[] {
    const result: string[] = ["/"];
    const walk = (dir: Directory, prefix: string): void => {
      for (const [name, node] of dir.contents) {
        const path = `${prefix}/${name}`;
        result.push(node instanceof Directory ? `${path}/` : path);
        if (node instanceof Directory) walk(node, path);
      }
    };
    walk(this.root, "");
    return result;
  }

  private usedBytes(): number {
    const files = new Set<File>();
    const walk = (dir: Directory): void => {
      for (const node of dir.contents.values()) {
        if (node instanceof File) files.add(node);
        else if (node instanceof Directory) walk(node);
      }
    };
    walk(this.root);
    for (const fd of this.handles.values()) files.add(fd.file);
    return Array.from(files).reduce((sum, file) => sum + Number(file.size), 0);
  }

  private handle(id: number | undefined): OpenFile | undefined {
    if (id === undefined || id === null) return undefined;
    const fd = this.handles.get(id);
    if (!fd) throw new HostFailure("invalid file handle");
    return fd;
  }

  private run(request: RunRequest): { code: number; chunks: Chunk[] } {
    // Resolve the supported command set before running any stage.
    for (const argv of request.stages) {
      if ((!argv[0] || !this.commands.has(argv[0]))) throw new HostFailure(`command '${argv[0]}' is not available in this playground`, 127);
      if (argv.some(arg => arg.includes("\0"))) throw new HostFailure("command arguments cannot contain NUL");
    }
    const chunks: Chunk[] = [];
    let outputBytes = 0;
    const emit = (stream: Chunk["stream"], bytes: Uint8Array): void => {
      outputBytes += bytes.length;
      if (outputBytes > OUTPUT_LIMIT) throw new HostFailure("command output exceeds 8 MiB");
      // Copy out of guest memory before it changes or grows.
      chunks.push({ stream, bytes: bytes.slice() });
    };
    let input: OpenFile = this.handle(request.input) ?? new OpenFile(new File(new Uint8Array()));
    let code = 0;
    for (const [index, argv] of request.stages.entries()) {
      const last = index === request.stages.length - 1;
      const pipe: Uint8Array[] = [];
      let pipeBytes = 0;
      const stdout = last
        ? this.handle(request.output) ?? new ConsoleStdout(bytes => emit("stdout", bytes))
        : new ConsoleStdout(bytes => {
          pipeBytes += bytes.length;
          if (pipeBytes > OUTPUT_LIMIT) throw new HostFailure("pipeline buffer exceeds 8 MiB");
          pipe.push(bytes.slice());
        });
      const stderr = this.handle(request.error) ?? new ConsoleStdout(bytes => emit("stderr", bytes));
      // Only terminal streams report a character device. Captures/pipes are not TTYs.
      if (!last || request.capture) {
        if (stdout instanceof ConsoleStdout) {
          stdout.fd_fdstat_get = () => ({ ret: 0, fdstat: new defs.Fdstat(defs.FILETYPE_REGULAR_FILE, 0) });
          stdout.fd_filestat_get = () => ({ ret: 0, filestat: new defs.Filestat(0n, defs.FILETYPE_REGULAR_FILE, 0n) });
        }
      }
      // Prevent fd_close in one instance from destroying a shared redirection.
      const fds = [input, stdout, stderr, new PreopenDirectory("/", this.root.contents)];
      const environment = Array.from(request.environment, ([key, value]) => `${key}=${value}`);
      const wasi = new WASI(argv, environment, fds);
      this.fixImports(wasi);
      const cwd = encoder.encode(request.cwd);
      const instance = new WebAssembly.Instance(this.module, {
        wasi_snapshot_preview1: wasi.wasiImport,
        parsh: {
          cwd_len: () => cwd.length,
          cwd_write: (ptr: number) => new Uint8Array(wasi.inst.exports.memory.buffer).set(cwd, ptr),
        },
      });
      const status = wasi.start(instance as Parameters<WASI["start"]>[0]);
      if (status !== 0) code = status;
      if (!last) {
        const bytes = new Uint8Array(pipeBytes);
        let offset = 0;
        for (const chunk of pipe) { bytes.set(chunk, offset); offset += chunk.length; }
        input = new OpenFile(new File(bytes));
      }
    }
    return { code, chunks };
  }

  private fixImports(wasi: WASI): void {
    const imports = wasi.wasiImport;
    const sizes = (entries: string[], count: number, size: number): number => {
      const view = new DataView(wasi.inst.exports.memory.buffer);
      view.setUint32(count, entries.length, true);
      view.setUint32(size, entries.reduce((sum, text) => sum + encoder.encode(text).length + 1, 0), true);
      return 0;
    };
    imports.args_sizes_get = (count, size) => sizes(wasi.args, count, size);
    imports.environ_sizes_get = (count, size) => sizes(wasi.env, count, size);
    imports.random_get = (ptr, length) => {
      const bytes = new Uint8Array(wasi.inst.exports.memory.buffer, ptr, length);
      for (let start = 0; start < length; start += 65536) crypto.getRandomValues(bytes.subarray(start, start + 65536));
      return 0;
    };
    const grow = (fd: number, size: bigint): void => {
      const file = wasi.fds[fd];
      if (file instanceof OpenFile && size > file.file.size && this.usedBytes() + Number(size - file.file.size) > FILE_LIMIT) {
        throw new HostFailure("virtual files exceed 32 MiB");
      }
    };
    const allocate = imports.fd_allocate!;
    imports.fd_allocate = (fd, offset, length) => { grow(fd, offset + length); return allocate(fd, offset, length); };
    const resize = imports.fd_filestat_set_size!;
    imports.fd_filestat_set_size = (fd, size) => { grow(fd, size); return resize(fd, size); };
    const write = imports.fd_write!;
    imports.fd_write = (fd, iovs, count, written) => {
      const file = wasi.fds[fd];
      if (file instanceof OpenFile) {
        const view = new DataView(wasi.inst.exports.memory.buffer);
        let size = 0n;
        for (let i = 0; i < count; i++) size += BigInt(view.getUint32(iovs + i * 8 + 4, true));
        grow(fd, file.file_pos + size);
      }
      return write(fd, iovs, count, written);
    };
    const pwrite = imports.fd_pwrite!;
    imports.fd_pwrite = (fd, iovs, count, offset, written) => {
      const view = new DataView(wasi.inst.exports.memory.buffer);
      let size = 0n;
      for (let i = 0; i < count; i++) size += BigInt(view.getUint32(iovs + i * 8 + 4, true));
      grow(fd, offset + size);
      return pwrite(fd, iovs, count, offset, written);
    };
  }
}
