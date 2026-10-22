import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { resolve, dirname } from "node:path";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const output = resolve(root, "website/public/wasm");
const result = spawnSync(
  "wasm-pack",
  [
    "build",
    resolve(root, "crates/parsh-wasm"),
    "--target",
    "web",
    "--release",
    "--out-dir",
    output,
    "--out-name",
    "parsh",
    "--locked",
  ],
  {
    cwd: root,
    stdio: "inherit",
    env: { ...process.env, WASM_PACK_CACHE: process.env.WASM_PACK_CACHE ?? resolve(root, "target/wasm-pack-cache") },
  },
);
if (result.error) {
  console.error("Install wasm-pack first: cargo install wasm-pack --locked");
  console.error(result.error.message);
}
process.exit(result.status ?? 1);
