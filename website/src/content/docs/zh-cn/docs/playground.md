---
title: "在线体验终端"
description: "在浏览器中运行 parsh、coreutils 和命令管道。"
---

[打开交互终端](/parsh/zh-cn/playground/)，运行由 Rust 编译为 WebAssembly 的真正 parsh 解释器与 uutils coreutils。

## 输入第一条命令

提示符 `$ ()` 已提供外层括号。输入 `echo hello` 并按 Enter 即可执行，也可以粘贴完整表达式后按 Enter：

```parsh
(echo "hello, parsh")
(let here (pwd))
(echo (str "working in " $here))
(| (sort fruits.txt) (uniq -c))
(|> results.txt (<| numbers.txt (sort -n)))
(cat results.txt)
```

命令直接打印输出。`(str hello)` 等表达式会显示其返回值（例如文本 `"hello"`）；nil 和命令退出状态不会显示。

## 命令与文件

以下 29 条命令使用真实的 uutils 实现：

`echo`、`printf`、`pwd`、`ls`、`cat`、`head`、`tail`、`sort`、`uniq`、`wc`、`seq`、`cut`、`tr`、`tee`、`nl`、`base64`、`sha256sum`、`basename`、`dirname`、`mkdir`、`rmdir`、`touch`、`cp`、`mv`、`rm`、`test`、`true`、`false`、`printenv`。

初始工作目录为 `/home/playground`，包含 `fruits.txt`、`numbers.txt` 和 `README.txt`，另提供 `/tmp`。会话初始化 `HOME`、`PWD`、`LANG` 和 `TERM`；`$args` 为空。`export` 修改传递给后续命令的环境变量。

文件操作、`cd`、显式 `glob`、命令捕获、管道及全部 parsh 重定向操作均使用同一个临时文件系统：

```parsh
(mkdir -p demo)
(cd demo)
(|> greeting.txt (echo hello))
(|>> greeting.txt (echo parsh))
(cp greeting.txt copy.txt)
(printf "%s\n" (glob *.txt))
(if (test -f copy.txt) (cat copy.txt) (echo missing))
(cd ..)
```

嵌套命令按 UTF-8 捕获标准输出，移除末尾 LF，并将结果保持为一个参数。被捕获表达式内部的显式输出重定向优先。文件和管道保留原始二进制字节。

浏览器管道逐阶段缓冲执行，返回最右侧非零状态，不提供原生实时流式处理或 SIGPIPE 行为。未提供文件或上游管道时，标准输入为 EOF。单次命令输出和每段管道缓冲上限为 8 MiB，虚拟文件总量上限为 32 MiB。首版不包含 `grep`、`sed`、`find` 或交互程序。

## 会话与按键

- Tab 补全命令、临时文件路径和 `$变量名`；多个匹配先补公共前缀，再按一次显示候选。例如输入 `cat fru` 后按 Tab 可补全 `fruits.txt`。
- Enter 检查整个编辑区，必要时补一个 `)` 后执行；输入未完成时换行，Ctrl+J 始终换行。输入 `)` 或粘贴都只编辑文本；粘贴批次先整批通过语法检查才会执行。
- 上下箭头移动行或召回历史；Ctrl+P/N 始终切换历史。召回时完整显示源码，光标停在最外层右括号前，可直接追加参数或按 Enter 重新执行。
- 支持 Emacs 风格的按词编辑、Ctrl+Y 粘回、Alt+Y 轮换、Ctrl+T 交换、Ctrl+L 清屏和 Ctrl+_ 撤销。Ctrl+R/S 搜索历史，Enter/Esc 接受但不执行，Ctrl+G 恢复草稿；Alt 键也可用 Esc 前缀。详见[交互编辑](/parsh/zh-cn/docs/interactive/)。浏览器保留的快捷键可能无法传入终端。
- Unicode 编辑、多行输入、完整脚本粘贴和批次错误恢复与原生版本共用同一套编辑模型。
- 清屏和空闲时 Ctrl+C 保留文件、变量和工作目录。
- 重置、刷新或离开页面会结束当前临时会话，浏览器后退缓存可能暂时保留页面。
- 执行时 Ctrl+C 会立即停止并重置整个会话。
- 单个表达式执行超过 5 秒会自动重置，清除文件、变量、函数、环境和历史。
- `exit` 和空输入时 Ctrl+D 结束会话，可重置重新开始。

要操作自己的文件并运行 Linux 程序，请[在 Linux 上使用 parsh](/parsh/zh-cn/docs/getting-started/)。
