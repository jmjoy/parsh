---
title: "基础库"
description: "不可变集合、JSON、原生 HTTP 请求、类型判断和数值函数。"
---

这些普通内置都支持 `(function name)` 和 `apply`。数据操作在原生终端和浏览器中共用实现，
HTTP 请求和后台任务要求原生能力。集合更新返回新值，需要更新绑定时显式使用 `set`。
签名中的方括号表示可选参数，并非列表字面值；`...` 表示可重复参数。

标量转换函数、`map/get`、`map/set`、`map/del`、`map/has`、`map/get-in`、
`map/set-in`、`list/first`、`list/get`、`list/slice`、`str/get`、`str/slice`、
`str/split`、`str/join`、`str/replace`、`bytes/slice`、`http/request`、
`http/get` 和 `http/post` 支持关键字参数。名称与文档中的签名一致。
`map/get`、`map/set`、`map/del` 和 `map/has` 的第一个参数名为 `map`。

```parsh
(dump (map/get {name parsh} missing &default unknown))
(dump (str/replace "a-b" &to "/" &from "-"))
```

## 模块命名空间

字符串、列表和映射操作使用 `模块/成员` 名称，例如 `str/join`、`list/append` 和 `map/get`。
它们都是普通内置函数：

```parsh
(dump (str/join [a b] ","))
(dump (list/append [a] b))
(dump (map/get {name parsh} name))
(dump (apply (function str/join) [[a b] ","]))
(dump (apply "str/join" [[a b] ","]))
```

构造器使用 `str`、`list` 和 `map`。
`str/join` 这样的名称仍是普通名称；其中的 `/` 只是名称或文本值中的一个字符，约定用来归类相关函数，
不是导入语法，也不是运行时的模块对象。单独的 `/` 仍然是除法内置。内置命名空间可以直接使用，无需导入；
目前尚不支持用户模块声明，可以通过 `load` 执行共享源文件。
[函数组合、嵌套更新、宏与脚本加载](/parsh/zh-cn/docs/lisp/)扩展了本页介绍的操作。

## 结构体字段

`(. value field...)` 可以读取用户定义或运行时结构体的一个或多个字段，包括 `CallResult`。
它接受结构体值和至少一个文本字段名，多个字段会从左到右链式读取。返回值保留字段的原始类型，包括 bytes、nil 和嵌套结构体；
字段不存在或中间值类型不正确时会报错。

```parsh
(defstruct Point x y)
(let point (Point #3 #4))
(let field x)
(dump (. $point $field))
(dump (. $point x))
(dump (apply "." [$point y]))
(dump (apply (function .) [$point x]))
```

结果依次为 `#3`、`#3`、`#4` 和 `#3`。`(. $point x y)` 等价于 `(. (. $point x) y)`。`.` 从左到右对参数求值，每个参数只求值一次，
字段读取本身不会覆盖 `$?`。原有 `Point.x` 访问器仍可使用。
嵌套字段读取和结构体定义详见[结构与宏](/parsh/zh-cn/docs/structs-and-macros/)。

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
布尔值不能转为数字，集合、表单、函数、nil 和`CallResult`都不是转换输入。
文本 `"#1"`、`"#false"` 不会重新按字面值读取。错误指向操作数，退出码为 `2`。

```parsh
(dump (int " 1.0 "))
(dump (float #1))
(dump (bool " false "))
(dump (apply (function int) ["1e2"]))
```

结果依次为 `#1`、`#1.0`、`#false`、`#100`。

## 类型判断

`typeof` 返回类型名称文本：`text`、`int`、`float`、`bool`、`list`、`map`、`form`、
`struct`、`bytes`、`function`、`command`、`nil`。
对应判定为 `text?`、`int?`、`float?`、`bool?`、`list?`、`map?`、`form?`、`struct?`、`bytes?`、
`function?`、`command?`、`nil?`。`number?` 同时接受整数和浮点数。
`callable?` 接受函数值和命令引用；文本名称即使可通过调用接口解析，也仍返回假。
`empty?` 只接受文本、列表和映射。

```parsh
(dump [(typeof (command echo)) (command? (command echo))
       (callable? (function +)) (callable? echo)])
```

结果为 `("command" #true #true #false)`。

判断严格检查运行时类型：裸词 `123` 是文本，因此 `(number? 123)` 为假，
`(number? #123)` 为真。普通命令在值位置返回 `CallResult`（`struct`），用 `CallResult?` 判断；`type` 继续用于查询命令解析。

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
(dump [(quot #-7 #3) (rem #-7 #3) (round #2.5) (round #-2.5)])
```

结果为 `(#-2 #-1 #3 #-3)`。`quot/rem` 仅接受 int，其他数值函数只接受 int 或 float；文本应先用 `int`、`float` 显式转换。
`min/max` 保留胜出数值的类型，并保留大整数与浮点混合比较的精度。
除零、整数溢出、取整结果超出 i64 和非有限数值仍是语言错误。

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
| `(list/range end)` / `(list/range start end [step])` | 不包含 end 的整数列表；start 默认为零，step 默认为一 |
| `(list/set xs index value)` | 替换索引处元素后的新列表 |
| `(list/insert xs index value)` | 在索引处元素之前插入 value 后的新列表 |
| `(list/del xs index)` | 删除索引处元素后的新列表 |
| `(list/enumerate xs)` | 从零开始的 `[index value]` 列表 |
| `(list/zip xs...)` | 将各列表对应元素配对为列表，长度取最短输入；零参数返回空列表 |

```parsh
(let original [a b])
(let updated (list/append $original [c d]))
(dump [$original $updated])
(dump (list/concat $original [c d]))
(dump (list/get $updated #-1))
(dump (list/slice $updated #1 #-1))
```

索引从 0 开始；负数从末尾计算，`#-1` 为最后一项。
索引和切片边界只接受 int；文本和整值 float 都会报错，需要时显式使用 `int`。单项读取越界时报错，除非提供默认值。
切片边界越界时截断，开始位置大于结束位置时返回空列表。

`set`、`insert` 和 `del` 使用相同的整数和负索引规则，越界时报错。
`insert` 还允许索引等于列表长度，表示追加元素；`#-1` 表示插入到最后一项之前。
空列表可以在索引零处插入。插入或替换的值始终作为一个元素保留，包括列表值。

`range` 只接受 int，支持负步长。步长为零时报错；步长方向无法到达终点时返回空列表。
每次调用立即生成最多 1,000,000 项，超过上限时在分配前报错。
`zip` 的每个输入都必须是列表，即使另一个输入为空；元素保留原始类型和嵌套结构。

```parsh
(dump (list/range #1 #6 #2))
(dump (list/range #5 #0 #-2))
(dump (list/insert [a b] #-1 [c d]))
(dump (list/enumerate [a b]))
(dump (list/zip [a b] [#1 #2 #3]))
```

结果依次为 `(#1 #3 #5)`、`(#5 #3 #1)`、`("a" ("c" "d") "b")`、
`((#0 "a") (#1 "b"))` 和 `(("a" #1) ("b" #2))`。

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
| `(str/starts-with text prefix)` / `(str/ends-with text suffix)` | 是否匹配字面前缀或后缀，返回布尔值 |
| `(str/lower text)` / `(str/upper text)` | Unicode 小写或大写文本 |

```parsh
(dump [(str/len "中文") (str/get "中文" #-1) (str/slice "中文abc" #1 #3)])
(dump (str/join (str/split "a,b,c" ",") " / "))
(dump (str/lines "a\r\nb\n"))
```

字符串索引和切片与列表共用负数、越界和默认值规则。
计数单位是 Unicode 标量值：组合重音或组合 emoji 可能占多个字符。
所有文本参数都要求文本；`str/join` 只接受文本元素，需要转换时显式使用 `str`。
分割保留中间和末尾的空字段；空分隔符沿用 Rust `split("")` 的规则，包含两端空字段。
替换使用 Rust `replace` 的规则，空模式会在字符之间及两端插入替换文本。

前后缀判断区分大小写，空前缀或后缀匹配任意文本，包括空文本。
大小写转换使用不依赖区域设置的 Unicode 规则，字符数量可能改变，
例如 `(str/upper Straße)` 返回 `"STRASSE"`。这些操作同样只接受文本。

## 数据变换

高阶列表函数把函数放在首位，并返回新值：

```parsh
(dump (list/map (fn (value) (str $value "!")) [a b]))
(dump (list/filter (fn (value) (eq $value yes)) [yes no yes]))
(dump (list/fold (fn (total value) (+ $total $value)) #0 [#1 #2 #3]))
```

| 调用 | 返回值 |
| --- | --- |
| `(list/map f xs)` | 依次调用 `f`，收集各项返回值 |
| `(list/filter predicate xs)` | 保留谓词返回真的原始元素 |
| `(list/find predicate xs)` | 谓词首次返回真时的原始元素；未命中返回 nil |
| `(list/fold f initial xs)` | 从左到右，将累加值和每个元素传给 `f` |
| `(list/any predicate xs)` | 首个真值处短路；空列表为假 |
| `(list/all predicate xs)` | 首个假值处短路；空列表为真 |
| `(list/reverse xs)` | 顺序反转后的新列表 |
| `(list/contains xs value)` | 按 `eq` 的类型和值相等规则判断成员关系 |
| `(list/index-of xs value)` | 首个按 `eq` 匹配的元素索引，从零开始；未命中返回 nil |
| `(list/sort xs)` | 稳定升序排列后的新列表；仅接受文本或数值元素 |
| `(list/unique xs)` | 按 `eq` 全局去重后的新列表，保留首次出现顺序 |

回调可以是函数值、外部命令引用或 `apply` 支持的文本名称。
`filter`、`find`、`any`、`all` 的回调必须返回布尔值，`CallResult`不能用作谓词结果。
空列表上的 `fold` 返回 `initial`。
`find` 在首次匹配时停止调用谓词，空列表返回 nil。

`sort` 接受文本列表或数值列表。整数与浮点数可以混合，比较时保留大整数精度；
数值相等的元素保留原有顺序和类型。文本按 Unicode 标量顺序排列，不依赖区域设置。
文本与数值混合、其他元素类型都会报错，空列表合法。
`unique` 和 `index-of` 接受任意值，使用与 `eq` 相同的结构化相等规则，
因此 `#1`、`#1.0` 和文本 `1` 仍然是三个不同的值。

映射支持浅合并和有序 entries 转换：

```parsh
(dump (map/merge {name parsh} {version #1}))
(dump (map/entries {name parsh version #1}))
(dump (map/from-entries [[name parsh] [version #1]]))
```

| 调用 | 返回值 |
| --- | --- |
| `(map/merge maps...)` | 浅合并后的新映射；零参数返回空映射 |
| `(map/entries m)` | 按插入顺序返回二元素 `[key value]` 列表组成的列表 |
| `(map/from-entries entries)` | 从文本键的二元素列表构造映射 |

后面的映射覆盖前面的值，同时保留原键的位置；新键追加到末尾。
嵌套值整体替换，不递归合并。`map/from-entries` 会拒绝格式错误的 pair 和重复键。

## 显式默认值严格求值

`map/get`、`list/get`、`list/first`、`str/get` 的显式默认值是普通实参。
参数个数合法后，实参从左到右各求值一次，再读取并选择结果。
即使命中已有条目，默认值中的副作用也会发生，错误也会终止调用：

```parsh
(let calls #0)
(dump (map/get {name parsh} name (do (set calls (+ $calls #1)) fallback)))
(dump [$calls])
```

结果为 `"parsh"` 和 `(#1)`。已有 nil 值的键仍然命中，返回 nil；默认值仍被求值。
此规则也适用于函数引用和 `apply`。函数声明中的 `&opt` 默认表达式则只在缺参时求值。

## JSON

`json/encode` 返回紧凑的 JSON 文本，接受标量、列表、映射和结构体；结构体字段会成为对象属性。
`json/decode` 接受 UTF-8 文本或 bytes，并返回对应的 parsh 值：对象变成映射，数组变成列表，`null` 变成 nil。

```parsh
(defstruct User name admin)
(let body (json/encode (User parsh #true)))
(let value (json/decode $body))
(dump (map/get $value name))
```

编码函数、命令引用、表单、bytes、状态和非有限数值都会报错。
结构体编码后的 JSON 再解码会得到映射，不会重建原结构体名称。JSON 对象保留输入顺序，重复键采用最后一个值。
解码数值时优先使用 i64，否则使用有限 f64，因此非常大的 JSON 整数可能损失精度。
JSON 错误的 `Error.kind` 为 `json`。

## Bytes

`bytes/` 函数操作 `{bytes #true}` 命令捕获和 HTTP 返回的原始 bytes：

| 调用 | 返回值 |
| --- | --- |
| `(bytes/len value)` | 字节长度。 |
| `(bytes/slice value start [end])` | 使用列表索引规则切片。 |
| `(bytes/concat values...)` | 拼接 bytes。 |
| `(bytes/from-text text)` / `(bytes/to-text bytes)` | UTF-8 转换，解码严格检查。 |
| `(bytes/base64-encode bytes)` / `(bytes/base64-decode text)` | 标准 Base64 转换。 |
| `(bytes/hex-encode bytes)` / `(bytes/hex-decode text)` | 小写 Hex 编码和严格解码。 |

索引与切片边界必须是 int。负数从末尾计算，越界边界被截断，反向范围返回空 bytes。
按字节切片可能拆开 UTF-8 序列，之后解码会报错。Hex 解码接受大小写数字，要求偶数长度，
拒绝空白与非十六进制文本。

```parsh
(let raw (bytes/hex-decode "00ff41"))
(dump [(bytes/len $raw) (bytes/base64-encode $raw)
       (bytes/to-text (bytes/slice $raw #-1))])
```

结果为 `(#3 "AP9B" "A")`。bytes 显示为 `<bytes N>`，不能隐式变成文本，需显式解码或编码。

## 调试输出

`dump` 使用值的显示形式打印任意 parsh 值，并追加换行。它返回 nil，且遵循其他内建输出相同的 stdout 重定向和捕获规则：

```parsh
(dump {name parsh items [#1 #true]})
(let result (<-result (printf "hello\n")))
(dump (CallResult.stdout $result))
```

需要把显示形式保存为文本时使用 `<-result`：

```parsh
(dump (stdout-text (<-result (dump {name parsh}))))
```

结果为 `"(map \"name\" \"parsh\")"`。普通 `(let value (dump ...))` 绑定的是 nil，
因为 `dump` 即使写入输出也仍返回 nil。

## HTTP

原生 parsh 通过 `http/request`、`http/get` 和 `http/post` 提供同步 HTTP 请求。
浏览器调用这些请求函数会报告能力不支持；`http/body-text` 只解码已有响应，属于共用的数据操作。

下面的示例假设指定 URL 有一个返回 JSON 的 HTTP 服务；请替换为实际服务地址：

```parsh
(let response
  (http/get "http://127.0.0.1:8080/data"
            {accept [application/json]}))
(dump (if (HttpResponse.ok $response)
    (json/decode (HttpResponse.body $response))
    (HttpResponse.status $response)))
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
4xx 或 5xx 响应仍作为数据返回；DNS、连接、协议和 body 超限错误会抛出 kind 为 `http` 的错误。
`ok` 仅在 2xx 状态时为真，每次请求的总超时为 30 秒，响应 body 上限为 8 MiB。
`HttpResponse` 是运行时结构体，通过 `HttpResponse.status` 等访问器查询字段，不能直接调用或重定义其构造器。

## 调用、结果与任务

`command`、`command?`、`callable?`、`bytes?`、`stdout-text`、`stderr-text`、`exit-code`、
`signal`、`check` 和 `throw` 在原生与浏览器环境共用实现，支持 `function` 和 `apply`。
`<-out`、`<-err` 和 `<-result` 各接收一个括号表达式作为捕获主体。
`spawn`、`wait`、`job-status` 和 `cancel` 要求原生任务支持。
[执行结果与后台任务](/parsh/zh-cn/docs/execution/)
