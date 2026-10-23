---
title: "快速开始"
description: "在 Linux 上构建与运行 parsh。"
---

原生 parsh 需要 Linux 和当前稳定版 Rust 工具链。先从仓库构建：

```console
git clone https://github.com/jmjoy/parsh.git
cd parsh
cargo build --release
./target/release/parsh
```

运行 `cargo install --path crates/parsh` 可将它安装到 PATH。

## 运行方式

```console
parsh
parsh -c '(echo hello)'
parsh examples/hello.parsh world
```

不带参数启动时，如果标准输入是终端，parsh 会打开交互编辑器；否则从标准输入读取脚本。
`parsh -` 可以显式指定从标准输入读取。另提供 `--help` 和 `--version`。
脚本参数通过列表 `$args` 获取，其中不包含脚本文件名。
将可执行文件加入 PATH 后，脚本可以使用 `#!/usr/bin/env parsh` 作为首行。

命令行选项由 clap 解析。脚本路径或 `-c SOURCE` 之后的所有参数都会原样传入脚本，
包括 `--help` 和 `--`。如果脚本文件名以 `-` 开头，可使用 `parsh -- -script.parsh`。

## 第一次交互

在 `$ ()` 提示符中输入 `str "hello, " "parsh"` 并按 Enter，parsh 会补上外层右括号，显示 `"hello, parsh"`。输入未完成时 Enter 换行，Ctrl+J 始终换行。

```parsh
(let name world)
(str "hello, " $name)
(echo hello $name)
```

第一个表达式绑定变量。第二个返回文本，交互环境会显示它。第三个调用原生 `echo` 程序并输出到 stdout。脚本模式不会自动打印表达式返回值。

[浏览器终端](/parsh/zh-cn/playground/)支持语言表达式和 coreutils 命令；命令清单与临时文件说明见[支持范围](/parsh/zh-cn/docs/playground/)。
