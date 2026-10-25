---
title: "命令与组合"
description: "原生命令、管道、输出捕获与文件重定向。"
---

:::note[Linux 原生环境]
本页介绍 parsh 命令组合。浏览器终端支持 coreutils 子集、临时文件与缓冲管道，具体见[支持范围](/parsh/zh-cn/docs/playground/)。其他 Linux 程序需在原生环境运行。
:::

命令依次查找内置形式、命名函数和 PATH 中的程序。
外部程序直接启动，表达式不会交给另一个 Shell 解析。
命令名也可以来自文本变量，例如 `($EDITOR file.txt)`；
变量值作为一个完整的程序名使用，不会隐式拆分出参数。

`map` 用于构造映射；内置操作包括 `mget`、`mset`、`mdel`、`mhas`、`mkeys`、`mvals` 和 `mlen`，
首参均为映射值，例如 `$config` 或 `{name parsh}`。更新操作返回新映射，
使用 `set` 显式赋回才会更新变量。命令签名与示例见[映射](/parsh/zh-cn/docs/variables/#映射)。

使用 `type` 查询命令的解析结果，不会执行被查询的命令：

```parsh
(type type cd echo)
(defn greet (name) (echo hello $name))
(type greet)
(type -a echo)
(let kind (type -t greet))
(if (type -t optional-tool) (optional-tool))
```

`type` 接受多个名称，也支持列表参数，并提供类似 Bash 的选项：

- `-a`：显示所有匹配，包括按 PATH 顺序找到的所有可执行文件路径。
- `-t`：对每个匹配输出 `builtin`、`function` 或 `file`。
- `-p`：输出外部命令路径；内建命令和函数不输出内容。
- `-P`：强制搜索 PATH，即使存在同名内建命令或函数。
- `-f`：跳过函数查找。

选项可以组合，`--` 表示选项结束。不带参数时静默成功。
找不到名称时状态为 `1`，非法选项的状态为 `2`。
输出支持捕获和重定向。浏览器终端按宿主提供的命令名称识别外部命令，
`-p` 和 `-P` 返回这些名称。

```parsh
(echo (pwd))
(| (find . -name "*.rs") (sort))
(|> files.txt (| (find . -name "*.rs") (sort)))
(|>> log.txt (echo another entry))
(<| input.txt (sort))
(|2> errors.txt (grep needle missing.txt))
```

组合操作如下：

- `|`：将外部命令连接为管道。
- `|>`、`|1>`：创建或清空文件，并将标准输出重定向到该文件。
- `|>>`、`|1>>`：创建文件或追加内容，并将标准输出重定向到该文件。
- `<|`：从文件读取标准输入。
- `|2>`：创建或清空文件，并将标准错误重定向到该文件。
- `|2>>`：创建文件或追加内容，并将标准错误重定向到该文件。

重定向始终将路径作为第一个参数、表达式作为第二个参数。
重定向形式可以嵌套，也可以包裹整个管道。
首版管道的每个阶段必须是外部命令调用；函数和改变 Shell 状态的内置形式在管道外执行。

例如，将标准输出写入清空后的日志，并将标准错误追加到另一个日志：

```parsh
(|1> output.log
  (|2>> errors.log
    (make)))
```

顶层命令实时输出。外部命令或管道用作值时，会将标准输出按 UTF-8 捕获，
移除末尾的 LF 换行符，并将结果保持为一个参数。标准错误仍写入原来的目标。
二进制输出应使用管道或文件重定向。显式重定向优先于对该表达式输出的捕获。
