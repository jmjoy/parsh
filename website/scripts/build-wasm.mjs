import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { resolve, dirname } from "node:path";
import { copyFileSync, readFileSync, writeFileSync } from "node:fs";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const output = resolve(root, "website/public/wasm");
// wasm-pack downloads the wasm-bindgen binary into its own cache. Never point that
// cache at `target/`: Swatinem/rust-cache removes loose files from non-profile
// directories there before saving, which strips the binary and leaves behind an empty
// `target/wasm-pack-cache/wasm-bindgen-*` directory. wasm-pack treats an existing
// directory as a finished install, so every later run then fails with
// "wasm-bindgen binary does not exist".
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
  },
);
if (result.error) {
  console.error("Install wasm-pack first: cargo install wasm-pack --locked");
  console.error(result.error.message);
}
if (result.status !== 0) process.exit(result.status ?? 1);
const runner = spawnSync(
  "cargo",
  ["build", "--locked", "--release", "-p", "parsh-coreutils", "--target", "wasm32-wasip1"],
  { cwd: root, stdio: "inherit" },
);
if (runner.error) console.error(runner.error.message);
if (runner.status !== 0) process.exit(runner.status ?? 1);
const manifest = readFileSync(resolve(root, "crates/parsh-coreutils/Cargo.toml"), "utf8");
const commands = Array.from(manifest.matchAll(/^uu_(\w+) =/gm), match => match[1]).sort();
const revision = manifest.match(/rev = "([a-f0-9]+)"/)[1];
writeFileSync(resolve(output, "coreutils.json"), JSON.stringify({ revision, commands }));
const metadata = spawnSync(
  "cargo",
  ["metadata", "--locked", "--format-version", "1", "--filter-platform", "wasm32-wasip1"],
  { cwd: root, encoding: "utf8", maxBuffer: 16 * 1024 * 1024 },
);
if (metadata.status !== 0) {
  console.error(metadata.stderr);
  process.exit(metadata.status ?? 1);
}
const workspace = JSON.parse(metadata.stdout);
copyFileSync(resolve(workspace.target_directory, "wasm32-wasip1/release/parsh-coreutils.wasm"), resolve(output, "coreutils.wasm"));
const uucore = workspace.packages.find(pkg => pkg.name === "uucore");
copyFileSync(resolve(dirname(uucore.manifest_path), "../../LICENSE"), resolve(output, "COREUTILS-LICENSE.txt"));
