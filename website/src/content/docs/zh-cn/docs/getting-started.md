---
title: "快速开始"
description: "在 Linux 上构建与运行 parsh。"
---

原生 parsh 需要 Linux 和当前稳定版 Rust 工具链。先从仓库构建：

```console
git clone https://github.com/jmjoy/parsh.git
cd parsh
cargo build --locked --release
./target/release/parsh
```

运行 `cargo install --path crates/parsh` 可将它安装到 PATH。

## 运行方式

```console
parsh
parsh -c '(echo hello)'
parsh -c '(printf "%s\n" $args)' first 'two words'
parsh examples/hello.parsh world
parsh - first < examples/hello.parsh
```

不带参数启动时，如果标准输入是终端，parsh 会打开交互编辑器；否则从标准输入读取脚本。
`parsh -` 可以显式指定从标准输入读取。另提供 `--help` 和 `--version`。
脚本参数通过列表 `$args` 获取，其中不包含脚本文件名。
将可执行文件加入 PATH 后，脚本可以使用 `#!/usr/bin/env parsh` 作为首行。

命令行选项由 clap 解析。脚本路径或 `-c SOURCE` 之后的所有参数都会原样传入脚本，
包括 `--help` 和 `--`。如果脚本文件名以 `-` 开头，可使用 `parsh -- -script.parsh`。

## AI 助手指南

原生二进制内置与当前版本对应的英文指南。可以先列出主题，再按任务需要读取。

```console
parsh skills
parsh skills get syntax
parsh skills get commands execution --full
parsh skills get --all --json
```

`parsh skills` 默认执行 `list`。`get` 输出 Markdown，`--full` 会附带已有的相关示例源码。
`path` 输出逻辑资源标识，例如 `embedded://parsh/skills/syntax`，用 `get` 读取对应指南。
`list`、`get` 和 `path` 均支持 `--json`。
要运行名为 `skills` 的脚本文件，请使用 `parsh ./skills`。

## 启动配置

交互式 parsh 在显示第一个提示符前执行 `config.parsh`。优先使用绝对路径
`$XDG_CONFIG_HOME/parsh/config.parsh`，其次使用绝对路径 `$HOME/.config/parsh/config.parsh`。
两个环境变量都没有提供绝对路径时，跳过自动配置。

parsh 首次创建 `parsh` 配置目录时，会写入带注释示例的模板，并向 stderr 打印路径。
目录已存在时，可以自行创建或删除 `config.parsh`。

配置文件使用普通 Parsh 代码。下面的示例为会话设置环境变量并定义命令。

```parsh
(export EDITOR vi)
(defn hello () (echo "hello from parsh"))
```

`-c`、脚本文件和从 stdin 读取源码的方式默认跳过配置。
在命令或脚本前加 `--config FILE` 可以指定配置文件，也可用 `--no-config` 跳过配置。
`FILE` 的相对路径以启动工作目录为基准。两个选项互斥。

```console
parsh --no-config
parsh --config ./settings.parsh -c '(hello)'
```

执行顺序和错误处理见[交互配置](/parsh/zh-cn/docs/interactive/#配置)。

## 第一次交互

在 `$ ()` 提示符中输入 `dump (str "hello, " "parsh")` 并按 Enter，parsh 会补上外层右括号，显示 `"hello, parsh"`。输入未完成时 Enter 换行，Ctrl+J 始终换行。

```parsh
(let name world)
(dump (str "hello, " $name))
(echo hello $name)
```

第一个表达式绑定变量。第二个通过 `dump` 显式打印文本。第三个调用原生 `echo` 程序并输出到 stdout。交互与脚本模式都不会自动打印返回值。

捕获命令输出时，默认会去掉首尾 Unicode 空白。如果需要保留精确输出，请使用 `{trim #false}`：

```parsh
(dump (<-out printf " hello\n"))
(dump (<-out {trim #false} printf " hello\n"))
```

这两次调用分别显示 `"hello"` 和 `" hello\n"`。

[浏览器终端](/parsh/zh-cn/playground/)支持语言表达式和 coreutils 命令；命令清单与临时文件说明见[支持范围](/parsh/zh-cn/docs/playground/)。

[示例脚本](/parsh/zh-cn/docs/examples/)提供集合、函数、命令结果和后台任务等完整程序。
