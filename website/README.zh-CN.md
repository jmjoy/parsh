# parsh 网站

[English](README.md) | 简体中文

[中文网站](https://jmjoy.github.io/parsh/zh-cn/)包含在线命令体验终端和
[文档](https://jmjoy.github.io/parsh/zh-cn/docs/)，另提供
[English](https://jmjoy.github.io/parsh/en/)。

体验终端运行由 Rust 编译的 WebAssembly 解释器及 29 条真实 uutils coreutils 命令，
支持临时文件、捕获、管道与重定向。函数、宏、集合、JSON、bytes、完整执行结果和异常处理与原生共用实现；
HTTP 请求与后台任务要求原生环境。[示例指南](https://jmjoy.github.io/parsh/zh-cn/docs/examples/)列出了可以在浏览器运行的脚本。

## 开发

需要 Node.js 24+、pnpm 12.4.2、稳定版 Rust、wasm-pack 0.13.1 和 agproc。
在仓库根目录执行：

```console
rustup target add wasm32-unknown-unknown wasm32-wasip1
cargo install wasm-pack --version 0.13.1 --locked
pnpm install --frozen-lockfile
agproc skills
agproc start website
```

在 `http://127.0.0.1:4321` 打开 `/parsh/en/` 或 `/parsh/zh-cn/`。
agproc 构建 WASM 和静态网站并管理预览；修改后执行 `agproc restart website`，结束后用 `agproc stop website` 停止。

```console
pnpm check
pnpm --filter parsh-website exec playwright install chromium
pnpm test
```

浏览器测试使用已构建的生产产物，并复用正在运行的本地预览；没有服务时 Playwright 配置会自行启动预览。
`pnpm build` 单独构建 WASM 和静态网站，`pnpm wasm:build` 只构建 WASM 资源。
完整检查与目标平台说明见[开发指南](src/content/docs/zh-cn/docs/development.md)。

## 部署

GitHub Actions 检查 PR，并将 `master` 或手动触发时通过检查的构建部署到 Pages。
首次部署需在 **Settings → Pages → Source** 中选择 **GitHub Actions**。
