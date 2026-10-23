# parsh 示例

[English](README.md) | 简体中文

在仓库根目录使用原生 parsh 运行：

```console
cargo run -- examples/hello.parsh world
cargo run -- examples/while.parsh
cargo run -- examples/files.parsh
```

将 `parsh` 加入 PATH 后，也可以直接运行，例如 `./examples/while.parsh`。
每个脚本都在开头注明运行方式，并显式打印结果。

| 脚本 | 演示内容 |
| --- | --- |
| [hello.parsh](hello.parsh) | 脚本参数、函数和命令条件 |
| [variables.parsh](variables.parsh) | 变量绑定、字符串插值、原样字符串、列表、`get` 和环境变量导出 |
| [arithmetic.parsh](arithmetic.parsh) | 累加、四则运算、数值比较和文本相等判断 |
| [functions.parsh](functions.parsh) | 递归、词法作用域、不同类型的返回值和函数输出捕获 |
| [for.parsh](for.parsh) | 列表遍历、`continue`、`break`、外层绑定和嵌套循环 |
| [while.parsh](while.parsh) | 条件循环、`continue`、带返回值的 `break` 和最后一轮的返回值 |
| [pipelines.parsh](pipelines.parsh) | 流式管道、输出捕获和命令输出参与运算 |
| [files.parsh](files.parsh) | 工作目录、glob、文件重定向、命令失败处理和清理 |

示例使用标准 Linux coreutils 命令。`files.parsh` 通过 `mktemp -d` 创建临时目录，
将演示文件写入其中，并在成功结束时删除该目录。
