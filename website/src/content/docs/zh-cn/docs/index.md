---
title: "认识 parsh"
description: "一个围绕 S 表达式构建的 Linux Shell。"
---

**parsh** 意为 **parenthesis shell**。它是一个命令行与脚本共用同一种小巧 Lisp 方言的 Linux Shell。

提示符是一个左括号。输入 `str hello)` 即可求值 `(str hello)`。Enter 插入换行，闭合最外层括号才会提交表达式。

```parsh
(let name world)
(defn greet (who)
  (str "hello, " $who))
(greet $name)
```

交互环境会显示返回值 `"hello, world"`。脚本使用完全相同的语法；在 Linux 上，表达式还可以启动真实程序、构建管道并重定向文件。

## 从这里开始

- [在 Linux 上开始使用](/parsh/zh-cn/docs/getting-started/)，运行你自己的命令。
- [在线体验语言](/parsh/zh-cn/playground/)，无需安装。
- [了解语法](/parsh/zh-cn/docs/syntax/)，逐步学习函数与命令组合。

## 当前范围

parsh 目前是早期实现，版本为 0.1.0。原生版本支持 Linux 前台任务，包括交互程序与管道。交互式 Tab 支持命令、路径和变量补全。后台任务、宏和持久化配置属于后续工作。浏览器终端提供语言核心、29 条 coreutils 命令、临时文件和命令组合。

使用[木兰宽松许可证，第 2 版](https://github.com/jmjoy/parsh/blob/master/LICENSE)。
