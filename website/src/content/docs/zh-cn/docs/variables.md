---
title: "变量与作用域"
description: "变量绑定、环境变量、列表与显式展开。"
---

裸词表示文本：`123`、`true`、`false` 和 `nil` 都没有特殊的字面量含义。
双引号字符串支持 `\"`、`\\`、`\n`、`\r` 和 `\t` 转义，不支持变量插值。
所有变量引用与绑定都使用 `$` 前缀，包括函数参数、循环绑定和环境变量设置。

```parsh
(let $name "a friend")
(echo hello $name)
(set $name world)
(echo (str "hello, " $name))
(export $EDITOR vim)
```

`let` 在当前词法作用域中绑定变量。`set` 更新最近的已有绑定，或已有的环境变量；
名称未定义时会报错。查找不到局部变量时，会继续查找继承的环境变量。
`export` 为后续子进程设置环境变量，并更新已有的同名局部绑定。
环境变量与工作目录的修改属于当前 parsh 会话。

```parsh
(let $files (list "a file.rs" other.rs))
(printf "<%s>\n" $files)
(for $file (glob *.rs)
  (echo $file))
```

列表作为外部命令参数时会展开一层。文本始终保持为一个参数，即使其中包含空格或换行。
`list` 构造列表，`str` 拼接标量值。`glob` 显式匹配文件，返回排序后的路径列表，
没有匹配项时返回空列表。普通裸词、字符串和变量值都不会隐式进行通配符展开、分词或波浪号展开。

`cd` 接受一个路径，不提供路径时使用 `$HOME`，并更新 `$PWD` 和 `$OLDPWD`。
相对命令路径、glob 模式和重定向路径均以当前工作目录为基准。
