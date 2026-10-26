---
title: "变量与作用域"
description: "变量绑定、环境变量、列表、映射与显式展开。"
---

在值的位置，裸词表示文本：`123`、`true`、`false` 和 `nil` 都没有特殊的字面量含义。
双引号字符串支持 `$name` 和 `$(...)` 插值，以及 `\"`、`\\`、`\n`、`\r`、`\t` 和 `\$` 转义。
单引号字符串完全保留原文，不插值也不处理转义。`${name}` 保持为普通文本。
三引号保留换行和缩进：`"""..."""` 支持相同的插值与转义，`'''...'''` 完全保留原文。
读取变量值时使用 `$name`；绑定和赋值目标使用裸名字，包括函数参数、循环绑定和环境变量设置。
变量名以字母或 `_` 开头，后续可包含字母、数字和 `_`。绑定目标不接受引号字符串或表达式。

有类型的标量使用 `#1`、`#1.0`、`#true`、`#false`、`#nil`。动态文本转换使用 `int`、`float`、`bool`，见[字面值语法](/parsh/zh-cn/docs/syntax/#标量字面值)与[转换接口](/parsh/zh-cn/docs/stdlib/#标量转换)。

所有普通数据采用值语义。读取变量、赋值和函数传参都会复制值，包括列表、映射及其嵌套内容。
集合更新返回新值，使用 `set` 显式赋回才会更新已有变量。

```parsh
(let name "a friend")
(echo hello $name)
(set name world)
(echo "hello, $name")
(export EDITOR vim)
```

`let` 在当前词法作用域中绑定变量。`set` 更新最近的已有绑定，或已有的环境变量；
名称未定义时会报错。查找不到局部变量时，会继续查找继承的环境变量。
`export` 为后续子进程设置环境变量，并更新已有的同名局部绑定。
环境变量与工作目录的修改属于当前 parsh 会话。

`get` 恰好接受一个文本变量名，参数也可以来自表达式；它按 `$name` 的相同规则查找并返回原始值：

```parsh
(get name)
(get (str na me))
(echo "hello, $(get name)_friend")
```

名称必须符合上面的变量命名规则，未定义时会报错。
`get` 保留列表、映射、数值、布尔值及 nil 的原始类型，放入字符串时才尝试转换为文本。
`get` 是保留的内置命令名。表达式插值和字面美元符的规则见[字符串](/parsh/zh-cn/docs/syntax/#字符串与插值)。

```parsh
(let files (list "a file.rs" other.rs))
(printf "<%s>\n" $files)
(for file (glob *.rs)
  (echo $file))
```

列表作为外部命令参数时会展开一层。文本始终保持为一个参数，即使其中包含空格或换行。
`list` 构造列表，`str` 拼接标量值；数值使用与命令参数、环境变量相同的格式转换为文本。
`glob` 显式匹配文件，返回排序后的路径列表，
没有匹配项时返回空列表。普通裸词、字符串和变量值都不会隐式进行通配符展开、分词或波浪号展开。

`cd` 接受一个路径，不提供路径时使用 `$HOME`，并更新 `$PWD` 和 `$OLDPWD`。
相对命令路径、glob 模式和重定向路径均以当前工作目录为基准。

## 映射

`map` 接受交替的文本键和值，`(map)` 创建空映射。
键可以是裸词、字符串、文本变量或返回文本的表达式，其他类型会报错。
值保留原始类型，可以包含列表或嵌套映射。键和值从左到右各求值一次。
参数个数为奇数时，在求值前报错；重复键在第二次出现的位置报错，不再求值该键对应的值。

```parsh
(let config (map name parsh version #1 nested (map enabled #true)))
(let updated (mset $config name new-name))
(mget $config name)
(mget $updated name)
(mget $config missing fallback)
(mkeys $config)
(set config (mdel $updated version))
```

所有 `m` 命令的首参都是映射值，可以是 `$config`、映射字面量或返回映射的表达式。
裸词 `config` 表示文本，会报类型错误。上例中 `updated` 包含新名字，
`config` 保持原值，直到最后的 `set` 才更新。

| 命令 | 结果 |
| --- | --- |
| `(mget $config key)` | 返回原始类型的值；缺键时报错 |
| `(mget $config key default)` | 总会求值 `default`，缺键时返回它 |
| `(mset $config key value)` | 返回新增或覆盖该值后的新映射 |
| `(mdel $config key)` | 返回删除该键后的新映射；缺键时报错 |
| `(mhas $config key)` | 返回键是否存在的布尔值 |
| `(mkeys $config)` | 按插入顺序返回文本键列表 |
| `(mvals $config)` | 按相同顺序返回值列表 |
| `(mlen $config)` | 返回整数条目数 |

已存在且值为 nil 的键仍然存在，返回 nil；显式默认值仍会求值。
默认值不会处理变量未定义或首参类型错误。
操作先检查参数个数，再从左到右各求值一次。`mget` 求值全部实参后检查映射并选择结果；
其余映射操作在首参通过映射类型检查后才继续求值键和值表达式。
操作使用这份独立的映射值；后续表达式即使修改来源变量，也不会改变已经取得的映射。
构造映射和 `mset` 的键类型错误时，不再求值对应的值表达式。

映射操作可以直接组合，无需先绑定中间结果：

```parsh
(mset {name parsh} version v1)
(mget (mset $config name other) name)
(mset (mset $config name other) version v2)
```

更新已有键保留原位置，新键追加到末尾。删除不改变其余键的相对顺序，重新插入追加到末尾。
映射按插入顺序显示为 `(map "name" "parsh")`；`eq` 比较内容时忽略该顺序。

`mset` 和 `mdel` 返回新映射，输入值保持原值。例如 `(let copy $config)` 之后，
`(set config (mset $config name changed))` 只更新 `config`，`copy` 及其嵌套数据保持原值。
函数可以用 `set` 更新捕获的外层绑定，同名形参则是独立绑定。
函数返回更新后的映射时，调用者显式赋回才会更新自己的变量。
`mget` 和 `mvals` 返回的值也各自独立。
映射不能隐式转为文本：`str`、字符串插值、外部命令参数和环境变量值都不接受映射。
需要列表用于循环或命令参数时，显式调用 `mkeys` 或 `mvals`。
