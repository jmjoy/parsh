---
title: "基础库"
description: "不可变列表、Unicode 字符串、类型判断和数值函数。"
---

这些内置函数在原生终端和浏览器中共用实现，都支持 `(function name)` 和 `apply`。
集合更新返回新值，需要更新绑定时显式使用 `set`。
下列表格中的方括号表示可选参数，`...` 表示可重复参数。

## 列表

| 调用 | 返回值 |
| --- | --- |
| `(llen xs)` | 整数长度 |
| `(lfirst xs [default])` | 首项；空列表时使用默认值或报错 |
| `(lrest xs)` | 去掉首项的新列表，空列表仍返回空列表 |
| `(lget xs index [default])` | 索引处的值；越界时使用默认值或报错 |
| `(lappend xs value...)` | 追加各个元素后的新列表，列表元素保留嵌套 |
| `(lconcat xs...)` | 将列表依次拼接；零参数返回空列表 |
| `(lslice xs start [end])` | 不包含 end 的切片，省略 end 表示到末尾 |

```parsh
(let original [a b])
(let updated (lappend $original [c d]))
(list $original $updated)
(lconcat $original [c d])
(lget $updated -1)
(lslice $updated 1 -1)
```

索引从 0 开始；负数从末尾计算，`-1` 为最后一项。
索引只接受整数或整数字符串，不接受浮点数。单项读取越界时报错，除非提供默认值。
切片边界越界时截断，开始位置大于结束位置时返回空列表。

## 字符串

| 调用 | 返回值 |
| --- | --- |
| `(slen text)` | 按 Rust `chars()` 计算的整数长度 |
| `(sget text index [default])` | 一个字符的文本；越界时使用默认值或报错 |
| `(sslice text start [end])` | 按相同字符单位计算的切片 |
| `(ssplit text separator)` | 按字面分隔符拆分得到的文本列表 |
| `(sjoin texts separator)` | 用分隔符连接文本列表 |
| `(slines text)` | 行列表，移除 LF 或 CRLF，不保留末尾换行产生的空行 |
| `(strim text)` | 去掉两端 Unicode 空白 |
| `(sreplace text from to)` | 将所有不重叠的字面匹配替换为 to |
| `(scontains text part)` | 是否包含字面子串，返回布尔值 |

```parsh
(list (slen "中文") (sget "中文" -1) (sslice "中文abc" 1 3))
(sjoin (ssplit "a,b,c" ",") " / ")
(slines "a\r\nb\n")
```

字符串索引和切片与列表共用负数、越界和默认值规则。
计数单位是 Unicode 标量值：组合重音或组合 emoji 可能占多个字符。
所有文本参数都要求文本；`sjoin` 只接受文本元素，需要转换时显式使用 `str`。
分割保留中间和末尾的空字段；空分隔符沿用 Rust `split("")` 的规则，包含两端空字段。
替换使用 Rust `replace` 的规则，空模式会在字符之间及两端插入替换文本。

## 显式默认值严格求值

`mget`、`lget`、`lfirst`、`sget` 的显式默认值是普通实参。
参数个数合法后，实参从左到右各求值一次，再读取并选择结果。
即使命中已有条目，默认值中的副作用也会发生，错误也会终止调用：

```parsh
(let calls (+))
(mget {name parsh} name (do (set calls (+ $calls 1)) fallback))
(list $calls)
```

结果为 `"parsh"` 和 `(1)`。已有 nil 值的键仍然命中，返回 nil；默认值仍被求值。
此规则也适用于函数引用和 `apply`。函数声明中的 `&optional` 默认表达式则只在缺参时求值。

## 类型判断

`typeof` 返回类型名称文本：`text`、`int`、`float`、`bool`、`list`、`map`、`form`、
`struct`、`function`、`nil`、`status`。对应判定为 `text?`、`int?`、`float?`、`bool?`、
`list?`、`map?`、`form?`、`struct?`、`function?`、`nil?`、`status?`。
`number?` 同时接受整数和浮点数；`empty?` 判断文本、列表或映射是否为空，其他类型报错。

判断严格检查运行时类型：裸词 `123` 是文本，因此 `(number? 123)` 为假，
`(number? (+ 123))` 为真。普通命令在值位置会捕获成文本；`type` 继续用于查询命令解析。

## 基础数值函数

| 调用 | 返回值 |
| --- | --- |
| `(quot a b)` | 向零取整的整数商 |
| `(rem a b)` | 与上述商对应的余数，符号随被除数 |
| `(abs n)` | 绝对值，保留整数或浮点类型 |
| `(min n...)`、`(max n...)` | 至少一个参数，返回胜出数值，平局取首项 |
| `(floor n)` | 向下取整的 i64 |
| `(ceil n)` | 向上取整的 i64 |
| `(round n)` | 最接近的 i64，中点远离零 |

```parsh
(list (quot -7 3) (rem -7 3) (round 2.5) (round -2.5))
```

结果为 `(-2 -1 3 -3)`。`quot/rem` 仅接受整数或整数字符串，其他数值函数接受数值或数值文本。
`min/max` 保留胜出数值的类型，并保留大整数与浮点混合比较的精度。
除零、整数溢出、取整结果超出 i64 和非有限数值仍是语言错误。
