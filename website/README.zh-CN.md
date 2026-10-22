# parsh 网站

[English](README.md) | 简体中文

[中文网站](https://jmjoy.github.io/parsh/zh-cn/)包含在线命令体验终端和
[文档](https://jmjoy.github.io/parsh/zh-cn/docs/)，另提供
[English](https://jmjoy.github.io/parsh/en/)。

体验终端运行由 Rust 编译的 WebAssembly 解释器及 29 条真实 uutils coreutils
命令，支持临时文件、命令捕获、管道与重定向。

## 开发

网站开发需要 Node.js 24+、pnpm 12.4.2 和 wasm-pack 0.13.1。
以下命令均在项目根目录执行：

```console
rustup target add wasm32-unknown-unknown wasm32-wasip1
cargo install wasm-pack --version 0.13.1 --locked
pnpm install --frozen-lockfile
pnpm dev
```

在打印的本地地址打开 `/parsh/en/` 或 `/parsh/zh-cn/`。
`pnpm build` 构建 WASM 和静态网站；
`pnpm check` 检查类型。先运行
`pnpm --filter parsh-website exec playwright install chromium` 安装浏览器，
再用 `pnpm test` 测试生产产物。

## 部署

GitHub Actions 检查 PR，并将 `master` 上通过检查的构建部署到 Pages。
首次部署需在 **Settings → Pages → Source** 中选择 **GitHub Actions**。
