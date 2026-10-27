---
title: "基础库"
description: "不可变集合、JSON、原生 HTTP 请求、类型判断和数值函数。"
---

这些内置函数在原生终端和浏览器中共用实现，都支持 `(function name)` 和 `apply`。
集合更新返回新值，需要更新绑定时显式使用 `set`。
下列表格中的方括号表示可选参数，`...` 表示可重复参数。

## 模块命名空间

字符串、列表和映射操作使用 `模块/成员` 名称，例如 `str/join`、`list/append` 和 `map/get`。
它们都是普通内置函数：

```parsh
(str/join [a b] ",")
(list/append [a] b)
(map/get {name parsh} name)
(apply (function str/join) [[a b] ","])
(apply "str/join" [[a b] ","])
```

构造器 `str`、`list`、`map` 保留原名。旧的字母前缀操作名已移除，脚本需要改用模块名称。
`str/join` 这样的名称仍是普通名称；其中的 `/` 只是名称或文本值中的一个字符，约定用来归类相关函数，
不是导入语法，也不是运行时的模块对象。单独的 `/` 仍然是除法内置。内置命名空间可以直接使用，无需导入；
目前尚不支持用户模块声明和文件导入。

## JSON

`json/encode` 返回紧凑的 JSON 文本，接受标量、列表、映射和结构体；结构体字段会成为对象属性。
`json/decode` 接受 UTF-8 文本或 bytes，并返回对应的 parsh 值：对象变成映射，数组变成列表，`null` 变成 nil。

```parsh
(defstruct User name admin)
(let body (json/encode (User parsh #true)))
(let value (json/decode body))
(map/get $value name)
```

编码函数、命令、表单、bytes、状态和非有限数值都会报错。JSON 对象保留输入顺序；重复键采用最后一个值。

## HTTP

原生 parsh 通过 `http/` 提供同步 HTTP 请求；浏览器运行时目前会报告能力不支持。

```parsh
(let response
  (http/get "https://example.com/api"
            (map "accept" ["application/json"])))
(if (HttpResponse.ok $response)
    (json/decode (HttpResponse.body $response))
    (HttpResponse.status $response))
```

可用函数如下：

| 调用 | 返回值 |
| --- | --- |
| `(http/request method url [body] [headers])` | `HttpResponse`；自定义方法无 body 但有 headers 时使用 `#nil` 占位。 |
| `(http/get url [headers])` | GET 响应。 |
| `(http/post url body [headers])` | POST 响应。 |
| `(http/body-text response)` | 将响应 body 严格解码为 UTF-8 文本。 |

请求 body 接受 nil、文本或 bytes。Headers 是文本名称到文本列表的映射；响应 header 名称统一为小写，重复值保留在列表中。
`HttpResponse` 包含 `status`、`ok`、`headers`、bytes 类型的 `body` 和最终 `url`。
4xx 或 5xx 响应仍作为数据返回；DNS、连接、协议和 body 超限错误会抛出语言错误。

## 数据变换

高阶列表函数把函数放在首位，并返回新值：

```parsh
(list/map (fn (value) (str $value "!")) [a b])
(list/filter (fn (value) (eq $value yes)) [yes no yes])
(list/fold (fn (total value) (+ $total $value)) #0 [#1 #2 #3])
```

另外还有 `list/any`、`list/all`、`list/reverse` 和 `list/contains`。
谓词必须返回布尔值；空列表上的 `any` 返回假，`all` 返回真。

映射支持浅合并和有序 entries 转换：

```parsh
(map/merge {name parsh} {version #1})
(map/entries {name parsh version #1})
(map/from-entries [[name parsh] [version #1]])
```

后面的映射覆盖前面的值，同时保留原键的位置；新键追加到末尾。
`map/from-entries` 会拒绝格式错误的 pair 和重复键。

## Bytes

`bytes/` 函数操作命令捕获和 HTTP 返回的原始 bytes：

| 调用 | 返回值 |
| --- | --- |
| `(bytes/len value)` | 字节长度。 |
| `(bytes/slice value start [end])` | 使用列表索引规则切片。 |
| `(bytes/concat values...)` | 拼接 bytes。 |
| `(bytes/from-text text)` / `(bytes/to-text bytes)` | UTF-8 转换，解码严格检查。 |
| `(bytes/base64-encode bytes)` / `(bytes/base64-decode text)` | 标准 Base64 转换。 |
| `(bytes/hex-encode bytes)` / `(bytes/hex-decode text)` | 小写 Hex 编码和严格解码。 |

## 标量转换

每个转换函数恰好接受一个参数，先检查参数个数，再求值一次。
函数引用与 `apply` 使用相同规则。`int`、`float`、`bool` 是保留的普通内建命令名。

| 调用 | 接受的输入与结果 |
| --- | --- |
| `(int value)` | int、i64 范围内没有小数部分的 float，或按现有规则解析的数值文本。`(int "1e2")` 返回 `#100`，`(int #1.5)` 报错。 |
| `(float value)` | int、float，或直接按 f64 解析的数值文本。允许正常舍入，包括大整数精度损失；拒绝非有限结果。 |
| `(bool value)` | bool，或去掉首尾空白后恰好为小写 `true`、`false` 的文本。数字、nil 和其他类型报错。 |

数值文本去掉首尾空白后必须完整解析。`int` 遇到带小数点或指数的文本时，先解析成 f64，
再按舍入后的值判断是否为整值。整数文本必须在 i64 范围内；
`(float "9223372036854775808")` 则可以直接解析成有限浮点数。
布尔值不能转为数字，集合、表单、函数、nil 和命令状态都不是转换输入。
文本 `"#1"`、`"#false"` 不会重新按字面值读取。错误指向操作数，退出码为 `2`。

```parsh
(int " 1.0 ")
(float #1)
(bool " false ")
(apply (function int) ["1e2"])
```

结果依次为 `#1`、`#1.0`、`#false`、`#100`。

## 列表

| 调用 | 返回值 |
| --- | --- |
| `(list/len xs)` | 整数长度 |
| `(list/first xs [default])` | 首项；空列表时使用默认值或报错 |
| `(list/rest xs)` | 去掉首项的新列表，空列表仍返回空列表 |
| `(list/get xs index [default])` | 索引处的值；越界时使用默认值或报错 |
| `(list/append xs value...)` | 追加各个元素后的新列表，列表元素保留嵌套 |
| `(list/concat xs...)` | 将列表依次拼接；零参数返回空列表 |
| `(list/slice xs start [end])` | 不包含 end 的切片，省略 end 表示到末尾 |

```parsh
(let original [a b])
(let updated (list/append $original [c d]))
(list $original $updated)
(list/concat $original [c d])
(list/get $updated #-1)
(list/slice $updated #1 #-1)
```

索引从 0 开始；负数从末尾计算，`#-1` 为最后一项。
索引和切片边界只接受 int；文本和整值 float 都会报错，需要时显式使用 `int`。单项读取越界时报错，除非提供默认值。
切片边界越界时截断，开始位置大于结束位置时返回空列表。

## 字符串

| 调用 | 返回值 |
| --- | --- |
| `(str/len text)` | 按 Rust `chars()` 计算的整数长度 |
| `(str/get text index [default])` | 一个字符的文本；越界时使用默认值或报错 |
| `(str/slice text start [end])` | 按相同字符单位计算的切片 |
| `(str/split text separator)` | 按字面分隔符拆分得到的文本列表 |
| `(str/join texts separator)` | 用分隔符连接文本列表 |
| `(str/lines text)` | 行列表，移除 LF 或 CRLF，不保留末尾换行产生的空行 |
| `(str/trim text)` | 去掉两端 Unicode 空白 |
| `(str/replace text from to)` | 将所有不重叠的字面匹配替换为 to |
| `(str/contains text part)` | 是否包含字面子串，返回布尔值 |

```parsh
(list (str/len "中文") (str/get "中文" #-1) (str/slice "中文abc" #1 #3))
(str/join (str/split "a,b,c" ",") " / ")
(str/lines "a\r\nb\n")
```

字符串索引和切片与列表共用负数、越界和默认值规则。
计数单位是 Unicode 标量值：组合重音或组合 emoji 可能占多个字符。
所有文本参数都要求文本；`str/join` 只接受文本元素，需要转换时显式使用 `str`。
分割保留中间和末尾的空字段；空分隔符沿用 Rust `split("")` 的规则，包含两端空字段。
替换使用 Rust `replace` 的规则，空模式会在字符之间及两端插入替换文本。

## 显式默认值严格求值

`map/get`、`list/get`、`list/first`、`str/get` 的显式默认值是普通实参。
参数个数合法后，实参从左到右各求值一次，再读取并选择结果。
即使命中已有条目，默认值中的副作用也会发生，错误也会终止调用：

```parsh
(let calls #0)
(map/get {name parsh} name (do (set calls (+ $calls #1)) fallback))
(list $calls)
```

结果为 `"parsh"` 和 `(#1)`。已有 nil 值的键仍然命中，返回 nil；默认值仍被求值。
此规则也适用于函数引用和 `apply`。函数声明中的 `&optional` 默认表达式则只在缺参时求值。

## 类型判断

`typeof` 返回类型名称文本：`text`、`int`、`float`、`bool`、`list`、`map`、`form`、
`struct`、`bytes`、`function`、`nil`、`status`。对应判定为 `text?`、`int?`、`float?`、`bool?`、
`list?`、`map?`、`form?`、`struct?`、`bytes?`、`function?`、`nil?`、`status?`。
`number?` 同时接受整数和浮点数；`empty?` 判断文本、列表或映射是否为空，其他类型报错。

判断严格检查运行时类型：裸词 `123` 是文本，因此 `(number? 123)` 为假，
`(number? #123)` 为真。普通命令在值位置会捕获成文本；`type` 继续用于查询命令解析。

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
(list (quot #-7 #3) (rem #-7 #3) (round #2.5) (round #-2.5))
```

结果为 `(#-2 #-1 #3 #-3)`。`quot/rem` 仅接受 int，其他数值函数只接受 int 或 float；文本应先用 `int`、`float` 显式转换。
`min/max` 保留胜出数值的类型，并保留大整数与浮点混合比较的精度。
除零、整数溢出、取整结果超出 i64 和非有限数值仍是语言错误。


## 调用、结果与任务

`command`、`command?`、`callable?`、`bytes?`、`capture-call`、`stdout-text`、`stderr-text`、`exit-code`、`signal`、`check`、`throw` 在原生与浏览器环境共用实现；`spawn`、`wait`、`job-status`、`cancel` 要求原生任务支持。这些普通内置都可通过 `function` 和 `apply` 调用。签名及结构体见[执行结果与后台任务](/parsh/zh-cn/docs/execution/)。
