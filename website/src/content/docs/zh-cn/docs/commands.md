---
title: "命令与组合"
description: "原生命令、管道、输出捕获与文件重定向。"
---

:::note[Linux 原生环境]
本页介绍 parsh 命令组合。浏览器终端支持 coreutils 子集、临时文件与缓冲管道，具体见[支持范围](/parsh/zh-cn/docs/playground/)。其他 Linux 程序需在原生环境运行。
:::

命令依次查找内置形式、别名、宏、结构体构造器与成员、命名函数，以及 PATH 中的程序。
外部程序直接启动，表达式不会交给另一个 Shell 解析。
命令名也可以来自文本变量，例如 `($EDITOR file.txt)`；
变量值作为一个完整的程序名使用，不会隐式拆分出参数。

调用头接受文本命令名、函数值或显式 `command` 引用；数值、布尔和 nil 即使来自变量也不可调用。需要文本命令名时显式使用 `str`。`int`、`float`、`bool` 是保留内建命令。

使用 `type` 查询命令的解析结果，不会执行被查询的命令：

```parsh
(type type cd echo)
(defn greet (name) (echo hello $name))
(type greet)
(type -a echo)
(let kind (<-out (type -t greet)))
(dump (if (type -t optional-tool) (optional-tool)))
```

`type` 接受多个名称，也支持列表参数，并提供类似 Bash 的选项：

- `-a`：显示所有匹配，包括按 PATH 顺序找到的所有可执行文件路径。
- `-t` 对每个匹配输出 `builtin`、`macro`、`struct`、`function`、`alias` 或 `file`。
- `-p` 只输出外部命令的路径。
- `-P` 强制搜索 PATH，即使存在同名的语言命令。
- `-f` 跳过函数、别名、宏和结构体查找。

选项可以组合，`--` 表示选项结束。不带参数时静默成功。
找不到名称时状态为 `1`，非法选项的状态为 `2`。
输出支持捕获和重定向。浏览器终端按宿主提供的命令名称识别外部命令，
`-p` 和 `-P` 返回这些名称。

## 别名

用 `alias` 可以为带预设参数的单条命令取短名。调用时，新参数会追加在预设参数后。

```parsh
(alias ll (ls -lh))
(ll /tmp)
(| (ll /tmp) (head -n 5))
(alias up (cd ..))
(dump (alias))
(type ll)
(unalias ll)
```

别名可以指向外部程序、`cd` 等普通内建、用户函数或已有别名。定义时解析目标，
并从左到右求值预设参数。普通内建和函数收到的参数保留原有类型。外部程序在调用时按
当前 `PATH` 查找。

可以重新定义别名，也可以给外部程序定义同名别名。`(alias ls (ls --color=auto))`
会绑定外部 `ls`，直接调用时可用 `((command ls) /tmp)`。别名名称不能与内建、函数、宏或
结构体命令重复。只有普通可调用命令可以作为目标。

`(alias)` 返回按名称排序的映射，每个值是原始调用表单。`(unalias NAME)` 删除会话中的别名。
`type` 可以查看定义，Tab 可以补全名称。别名支持 `function`、`apply`、捕获和后台任务。
外部程序别名也能用作管道阶段。把常用别名写入 `config.parsh`，每次加载该文件时都会生效。
[启动配置](/parsh/zh-cn/docs/getting-started/#启动配置)

## 管道与重定向

```parsh
(echo (<-out (pwd)))
(| (find . -name "*.rs") (sort))
(out-> files.txt (| (find . -name "*.rs") (sort)))
(out->> log.txt (echo another entry))
(in<- files.txt (sort))
(err-> errors.txt
  (if (cat missing.txt) (echo found) (echo "read failure handled")))
```

组合操作如下：

- `|`：将外部命令连接为管道。
- `out->`：创建或清空文件，并将标准输出重定向到该文件。
- `out->>`：创建文件或追加内容，并将标准输出重定向到该文件。
- `in<-`：从文件读取标准输入。
- `err->`：创建或清空文件，并将标准错误重定向到该文件。
- `err->>`：创建文件或追加内容，并将标准错误重定向到该文件。

重定向始终将路径作为第一个参数、表达式作为第二个参数。
重定向形式可以嵌套，也可以包裹整个管道。
管道的每个阶段必须调用外部命令，调用头可以是文本名称、文本变量、`command` 引用，
或绑定外部程序的别名。显式引用可以绕过同名的语言命令。

```parsh
(let sorter (command sort))
(| (printf "b\na\n") ($sorter))
```

例如，将标准输出写入清空后的日志，并将标准错误追加到另一个日志：

```parsh
(out-> output.log
  (err->> errors.log
    (make)))
```

普通命令无论出现在顶层、赋值、参数还是插值中，都向当前目标输出并返回 `CallResult`。
结果的 stdout/stderr 默认是 `#nil`；需要文本时显式使用 `<-out` 或 `<-err`，
需要两路输出和完整结果时使用 `<-result`。捕获文本默认裁剪首尾空白；
前置 `{trim #false}` 保留原文，`{bytes #true}` 返回不裁剪的原始 bytes。
显式文件重定向优先于捕获，管道和文件保留二进制字节。

捕获运算符可以直接包裹管道表达式。

```parsh
(let count (int (<-out (| (find . -name "*.rs") (wc -l)))))
```

完整结果、原始字节捕获、`$?`、异常和后台执行见[执行结果与后台任务](/parsh/zh-cn/docs/execution/)。
