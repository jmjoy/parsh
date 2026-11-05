---
title: "开发指南"
description: "原生与可移植版本测试、WASM 构建和 agproc 网站预览。"
---

在仓库根目录执行原生与可移植版本检查：

```console
cargo fmt --all --check
cargo test --locked
cargo test --locked --no-default-features --lib
cargo clippy --locked --all-targets -- -D warnings
cargo clippy --locked --no-default-features --lib -- -D warnings
```

测试覆盖真实子进程、大输出管道、重定向、词法函数、集合、JSON、本地 HTTP 服务、
执行结果、异常，以及后台任务隔离与取消。Linux PTY 测试验证粘贴、历史、Unicode 编辑、
前台任务中断、终端恢复，以及队列源码与子进程 stdin 的隔离。

修改文档涉及的语言功能时，也应运行对应的[示例脚本](/parsh/zh-cn/docs/examples/)，
例如 `cargo run -- examples/execution.parsh`。

## 网站与 WASM

需要 Node.js 24+、pnpm 12.4.2、当前稳定版 Rust、wasm-pack 0.13.1 和 agproc。
在仓库根目录安装构建依赖：

```console
rustup target add wasm32-unknown-unknown wasm32-wasip1
cargo install wasm-pack --version 0.13.1 --locked
pnpm install --frozen-lockfile
```

仓库的 `agproc.toml` 负责构建生产网站，并在 4321 端口管理预览服务。
管理服务前先加载项目指南：

```console
agproc skills
agproc start website
```

打开 `http://127.0.0.1:4321/parsh/en/` 或 `http://127.0.0.1:4321/parsh/zh-cn/`。
修改 Rust、网站代码或文档后，运行 `agproc restart website`，重新构建 WASM 和网站并更新预览。
`agproc start` 不会重新构建已运行的服务；`agproc ps` 查看状态，结束后用 `agproc stop website` 停止。

检查网站，并用生产产物执行浏览器测试：

```console
pnpm check
pnpm --filter parsh-website exec playwright install chromium
pnpm test
```

Playwright 在本地复用已经运行的预览服务；没有服务时，其配置会自行启动预览，包括 CI 环境。
测试不会构建网站。只需构建静态生产产物时使用 `pnpm build`；
`pnpm wasm:build` 仅重新构建浏览器解释器和 coreutils 资源。

工作流还会按目标平台检查 Rust 包：

```console
cargo clippy --locked -p parsh-coreutils --target wasm32-wasip1 -- -D warnings
cargo clippy --locked -p parsh-wasm --target wasm32-unknown-unknown -- -D warnings
```

生成的 WASM、依赖、构建目录和测试产物不提交到 Git。GitHub Actions 在 PR 上执行这些检查，
在 `master` 推送或手动触发后部署通过检查的构建。CI 优先使用已安装的 Chrome/Chromium，
没有可用浏览器时安装 Playwright headless shell。首次部署需将仓库 Pages 来源设置为 **GitHub Actions**。

浏览器包关闭 `parsh` 库默认的 `native` feature。解析、求值、编辑、历史和布局与 Linux 共用；
可注入的执行后端在 Worker 中运行 WASI coreutils，并提供临时文件系统。
`parsh-coreutils` 固定 uutils 提交，首次构建会下载上游依赖。
