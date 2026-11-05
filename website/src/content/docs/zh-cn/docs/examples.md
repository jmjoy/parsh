---
title: "示例脚本"
description: "按主题运行仓库示例，了解原生与浏览器的支持范围。"
---

仓库的 [`examples/`](https://github.com/jmjoy/parsh/tree/master/examples) 提供可以直接运行的完整脚本。
在仓库根目录执行：

```console
cargo run -- examples/hello.parsh world
cargo run -- examples/stdlib.parsh
cargo run -- examples/execution.parsh
```

脚本通过 `echo`、`printf` 和 `dump` 显式输出，文件模式不会自动显示返回值。
`dump` 使用 REPL 的值显示格式，例如 `(dump [#1 #true])` 输出 `(#1 #true)`。

## 基础语法

| 脚本 | 演示内容 | 运行环境 |
| --- | --- | --- |
| [hello.parsh](https://github.com/jmjoy/parsh/blob/master/examples/hello.parsh) | 脚本参数、命名函数和命令条件 | 仅原生 |
| [variables.parsh](https://github.com/jmjoy/parsh/blob/master/examples/variables.parsh) | 变量绑定、插值、原样字符串、列表、get 和 export | 原生与浏览器 |
| [literals.parsh](https://github.com/jmjoy/parsh/blob/master/examples/literals.parsh) | # 类型字面值和 int/float/bool 显式转换 | 原生与浏览器 |
| [arithmetic.parsh](https://github.com/jmjoy/parsh/blob/master/examples/arithmetic.parsh) | 四则运算、数值比较和文本相等判断 | 原生与浏览器 |

## 函数与控制流

| 脚本 | 演示内容 | 运行环境 |
| --- | --- | --- |
| [functions.parsh](https://github.com/jmjoy/parsh/blob/master/examples/functions.parsh) | 递归、词法作用域、闭包、可选与变长参数、return 和 apply | 原生与浏览器 |
| [for.parsh](https://github.com/jmjoy/parsh/blob/master/examples/for.parsh) | 列表遍历、每轮作用域、continue、break 和嵌套循环 | 原生与浏览器 |
| [while.parsh](https://github.com/jmjoy/parsh/blob/master/examples/while.parsh) | 条件循环、外层绑定和循环返回值 | 原生与浏览器 |
| [structs.parsh](https://github.com/jmjoy/parsh/blob/master/examples/structs.parsh) | 构造器、访问器、谓词、嵌套和值相等 | 原生与浏览器 |
| [macros.parsh](https://github.com/jmjoy/parsh/blob/master/examples/macros.parsh) | 引用表单、准引用、摊平、可选与变长参数和调用方作用域 | 原生与浏览器 |

## 集合与数据

| 脚本 | 演示内容 | 运行环境 |
| --- | --- | --- |
| [stdlib.parsh](https://github.com/jmjoy/parsh/blob/master/examples/stdlib.parsh) | 不可变列表、Unicode 文本、高阶变换和数值辅助函数 | 原生与浏览器 |
| [maps.parsh](https://github.com/jmjoy/parsh/blob/master/examples/maps.parsh) | 不可变更新、默认值严格求值、有序键、合并和 entries 转换 | 原生与浏览器 |
| [json.parsh](https://github.com/jmjoy/parsh/blob/master/examples/json.parsh) | 结构体编码、有类型的解码、字节输入和 JSON 错误 | 原生与浏览器 |
| [bytes.parsh](https://github.com/jmjoy/parsh/blob/master/examples/bytes.parsh) | 二进制值、UTF-8、切片、拼接、Base64 和 Hex | 原生与浏览器 |

## 命令与执行

| 脚本 | 演示内容 | 运行环境 |
| --- | --- | --- |
| [pipelines.parsh](https://github.com/jmjoy/parsh/blob/master/examples/pipelines.parsh) | 管道、文本捕获和命令边界上的数值转换 | 原生与浏览器 |
| [files.parsh](https://github.com/jmjoy/parsh/blob/master/examples/files.parsh) | 临时目录、glob、重定向和 finally 清理 | 仅原生 |
| [execution.parsh](https://github.com/jmjoy/parsh/blob/master/examples/execution.parsh) | 命令引用、完整结果、$?、原始捕获、check 和异常 | 原生与浏览器 |
| [tasks.parsh](https://github.com/jmjoy/parsh/blob/master/examples/tasks.parsh) | 后台执行、状态隔离、wait、job-status 和取消 | 仅原生 |
| [http.parsh](https://github.com/jmjoy/parsh/blob/master/examples/http.parsh) | 原生 HTTP 响应、headers、字节 body 和 JSON 解码 | 仅原生 |

## 浏览器中的运行方式

标注“原生与浏览器”的脚本可以粘贴到[在线体验终端](/parsh/zh-cn/playground/)后按 Enter 执行。
每个脚本使用新会话，避免顶层 `let` 名称重复。浏览器没有脚本文件入口，`$args` 为空，
管道按阶段缓冲执行；原生环境使用流式管道。
`hello.parsh` 依赖脚本参数与仓库路径，`files.parsh` 使用 `mktemp`，
`tasks.parsh` 使用原生后台任务，`http.parsh` 发送原生 HTTP 请求。
详细命令清单和限制见[体验终端支持范围](/parsh/zh-cn/docs/playground/)。

## 文件、任务与 HTTP

`files.parsh` 通过 `mktemp -d` 创建临时目录，并在 `finally` 中恢复工作目录和删除文件，
主体发生运行错误时仍会执行清理。`tasks.parsh` 演示启动后立即取消 `sleep 30`，无需等待完整的 30 秒。

HTTP 示例只在传入 URL 时发送请求：

```console
cargo run -- examples/http.parsh http://127.0.0.1:8080/data
```

将 URL 替换为可访问的服务。脚本打印响应信息，按 `Content-Type` 解码 JSON 或 UTF-8 文本；
HTTP 4xx/5xx 仍是数据，连接等传输失败则抛异常。

语法说明见[变量与作用域](/parsh/zh-cn/docs/variables/)、[基础库](/parsh/zh-cn/docs/stdlib/)、
[函数与控制流](/parsh/zh-cn/docs/control-flow/)和[执行结果与后台任务](/parsh/zh-cn/docs/execution/)。
