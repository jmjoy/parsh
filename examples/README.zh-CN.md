# parsh 示例

[English](README.md) | 简体中文

在仓库根目录使用原生 parsh 运行，每个脚本开头也注明了运行方式：

```console
cargo run -- examples/hello.parsh world
cargo run -- examples/functions.parsh
cargo run -- examples/maps.parsh
cargo run -- examples/execution.parsh
cargo run -- examples/tasks.parsh
```

将 `parsh` 加入 PATH 后，也可以直接执行，例如 `./examples/while.parsh`。
脚本不会自动打印表达式返回值；这些示例使用 `echo`、`printf` 或 `dump` 显式输出。
`dump` 可以直接显示列表、映射、结构体、函数和 bytes。

| 脚本 | 演示内容 | 运行环境 |
| --- | --- | --- |
| [hello.parsh](hello.parsh) | 脚本参数、命名函数和命令条件 | 仅原生 |
| [variables.parsh](variables.parsh) | 变量绑定、插值、原样字符串、列表、get 和 export | 原生与浏览器 |
| [literals.parsh](literals.parsh) | # 类型字面值和 int/float/bool 显式转换 | 原生与浏览器 |
| [arithmetic.parsh](arithmetic.parsh) | 四则运算、数值比较和文本相等判断 | 原生与浏览器 |
| [functions.parsh](functions.parsh) | 递归、词法作用域、闭包、可选与变长参数、return 和 apply | 原生与浏览器 |
| [for.parsh](for.parsh) | 列表遍历、每轮作用域、continue、break 和嵌套循环 | 原生与浏览器 |
| [while.parsh](while.parsh) | 条件循环、外层绑定和循环返回值 | 原生与浏览器 |
| [structs.parsh](structs.parsh) | 构造器、访问器、谓词、嵌套和值相等 | 原生与浏览器 |
| [macros.parsh](macros.parsh) | 引用表单、准引用、摊平、可选与变长参数和调用方作用域 | 原生与浏览器 |
| [stdlib.parsh](stdlib.parsh) | 不可变列表、Unicode 文本、高阶变换和数值辅助函数 | 原生与浏览器 |
| [maps.parsh](maps.parsh) | 不可变更新、默认值严格求值、有序键、合并和 entries 转换 | 原生与浏览器 |
| [json.parsh](json.parsh) | 结构体编码、有类型的解码、字节输入和 JSON 错误 | 原生与浏览器 |
| [bytes.parsh](bytes.parsh) | 二进制值、UTF-8、切片、拼接、Base64 和 Hex | 原生与浏览器 |
| [pipelines.parsh](pipelines.parsh) | 管道、文本捕获和命令边界上的数值转换 | 原生与浏览器 |
| [files.parsh](files.parsh) | 临时目录、glob、重定向和 finally 清理 | 仅原生 |
| [execution.parsh](execution.parsh) | 命令引用、完整结果、$?、原始捕获、check 和异常 | 原生与浏览器 |
| [tasks.parsh](tasks.parsh) | 后台执行、状态隔离、wait、job-status 和取消 | 仅原生 |
| [http.parsh](http.parsh) | 原生 HTTP 响应、headers、字节 body 和 JSON 解码 | 仅原生 |

浏览器列表示可以把脚本内容粘贴到[在线体验终端](https://jmjoy.github.io/parsh/zh-cn/playground/)后按 Enter 执行。
使用新的会话运行每个脚本，避免重复 `let` 绑定；浏览器没有脚本文件入口，`$args` 为空，管道按阶段缓冲执行。
`hello.parsh` 使用脚本参数和仓库路径；`files.parsh` 使用浏览器未提供的 `mktemp`；后台任务和 HTTP 请求要求原生环境。

示例中的外部命令来自标准 Linux coreutils。`files.parsh` 在临时目录中写文件，
通过 `finally` 恢复工作目录并删除临时文件，也适用于主体发生运行错误的情况。
`tasks.parsh` 会启动并取消一个 `sleep 30`，无需等待完整的 30 秒。

`http.parsh` 需要显式传入 HTTP/HTTPS URL；不传参数时只显示用法，不发送请求：

```console
cargo run -- examples/http.parsh http://127.0.0.1:8080/data
```

将示例 URL 替换为可访问的服务。脚本打印响应状态与 headers，按 `Content-Type` 解码 JSON 或 UTF-8 文本；
HTTP 4xx/5xx 仍作为响应数据处理。

完整语言说明及对应章节见[示例指南](https://jmjoy.github.io/parsh/zh-cn/docs/examples/)。
