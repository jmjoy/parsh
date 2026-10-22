---
title: "开发指南"
description: "原生测试、WASM 构建与网站开发。"
---

```console
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

测试覆盖真实子进程、大输出管道、文件重定向和词法作用域函数，
并通过 Linux PTY 验证粘贴、历史记录、Unicode 编辑、前台任务中断、
终端恢复，以及粘贴源码队列与子进程标准输入的隔离。

## 网站与 WASM

需要 Node.js 24+、pnpm 12.4.2、当前稳定版 Rust 和 wasm-pack 0.13.1。
以下命令均在项目根目录执行：

```console
rustup target add wasm32-unknown-unknown
cargo install wasm-pack --version 0.13.1 --locked
pnpm install --frozen-lockfile
pnpm dev
```

本地 URL 使用 `/parsh/en/` 或 `/parsh/zh-cn/`。开发命令先构建 WASM；修改 Rust 后，重新运行 `pnpm wasm:build` 并刷新页面。

```console
pnpm check
pnpm build
pnpm --filter parsh-website exec playwright install chromium
pnpm test
```

浏览器测试使用生产产物，先运行 `pnpm preview --port 4321`。也可使用 `agproc start website` 管理本地预览服务；修改后运行 `agproc restart website`。

WASM 产物、依赖、构建目录和测试输出不提交到 Git。GitHub Actions 在 PR 上执行检查，在 `master` 推送或手动触发后发布。首次部署需将仓库 Pages 来源设置为 **GitHub Actions**。

浏览器包关闭根库默认的 `native` feature。解析、求值、输入编辑、历史和布局与 Linux 共用；浏览器不提供文件系统或进程后端。
