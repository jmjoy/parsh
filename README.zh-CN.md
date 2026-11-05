# parsh

[English](README.md) | 简体中文

**把 Shell 放进括号里。**

parsh（**parenthesis shell**）是一个交互命令与脚本共用同一种小巧 Lisp 方言的 Linux Shell。
保留你熟悉的命令，让组合多一点结构。

项目使用 Rust 构建，以 S 表达式、显式的值和可组合命令为基础，目前处于早期开发阶段，源码开放。

[网站](https://jmjoy.github.io/parsh/zh-cn/) ·
[在线体验](https://jmjoy.github.io/parsh/zh-cn/playground/) ·
[中文文档](https://jmjoy.github.io/parsh/zh-cn/docs/)

## 命令行与脚本，同一种语言

在命令行中试好的表达式，可以直接放进脚本。变量、函数与控制流，始终遵循同一套规则。
命令调用就是表达式：

```parsh
(let name world)
(str "hello, " $name)

(defn greet (who)
  (str "hello, " $who))
(greet parsh)
```

交互环境会分别显示 `"hello, world"` 和 `"hello, parsh"`。
脚本使用相同语法，通过 `echo`、`printf` 或 `dump` 显式输出。

## 参数，保持完整

文本始终保持为一个参数，列表作为外部命令参数时展开一层。
没有隐式分词或通配符展开，无需猜测空格会如何影响参数：

```parsh
(let files (list "a file.rs" "b.rs"))
(list $files)
```

## 真实命令，结构化组合

在 Linux 上直接启动程序，用管道与重定向组合命令。
保留熟悉的工具，用新的语法来表达：

```parsh
(|> files.txt
  (| (find . -name "*.rs") (sort)))
```

## 逻辑和命令，一套语法

写给人，也写给 AI。算术、数据与控制流留在脚本里，程序、管道与重定向同样是表达式。
少一层需要猜的引号，生成的脚本读起来就是它执行的样子：

```parsh
(let count (int (| (ls) (wc -l))))
(echo "entries: $count; doubled: $(* $count #2)")
```

无需安装，即可在[在线体验终端](https://jmjoy.github.io/parsh/zh-cn/playground/)中运行 parsh 与 coreutils。
要在 Linux 上处理自己的文件并运行程序，请查看[快速开始](https://jmjoy.github.io/parsh/zh-cn/docs/getting-started/)。
[中文文档](https://jmjoy.github.io/parsh/zh-cn/docs/)包含语言参考和开发指南；
[示例目录](examples/README.zh-CN.md)提供按主题整理的完整脚本。

## 许可证

本项目采用《木兰宽松许可证，第 2 版》（Mulan Permissive Software License, Version 2）。
详情见 [LICENSE](LICENSE)。
