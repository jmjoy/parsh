---
title: "Lisp 工具与代码"
description: "函数组合、嵌套更新、预置宏、语法表单与脚本加载。"
---

这些能力扩展了[基础库](/parsh/zh-cn/docs/stdlib/)。普通函数在原生和浏览器环境中
都支持 `function` 与 `apply`。更新返回新值；检查参数个数后，显式实参从左到右各求值一次。
宏接收未求值的表单。

## 函数与序列

| 调用 | 结果 |
| --- | --- |
| `(identity value)` | 原值，保留类型 |
| `(constantly value)` | 接受任意参数、始终返回 value 的函数 |
| `(partial f args...)` | 调用 f 时在前面补入已捕获参数的函数 |
| `(compose functions...)` | 从右到左依次组合函数 |
| `(complement predicate)` | 对谓词的布尔结果取反的函数 |

这些适配器接受与 `apply` 相同的函数值、命令引用和文本名称。
名称在创建适配器时解析，保留当时的函数定义；捕获数据使用值语义。
返回值是普通闭包，可以传给后台任务。调用 `constantly` 返回的函数时，传入参数仍会求值。

`compose` 最右边的函数接收全部调用参数，其余函数各接收前一步的一个返回值。
`(compose)` 返回只接受一个参数的恒等函数。
`complement` 要求谓词返回布尔值；外部命令的 `CallResult` 也不能直接作为布尔谓词结果。

```parsh
(let add_ten (partial (function +) #10))
(dump (list/map $add_ten [#1 #2 #3]))
(dump ((compose (fn (n) (* $n #2)) (function +)) #2 #3))
(dump (list/filter (complement even?) [#1 #2 #3 #4]))
```

结果依次为 `(#11 #12 #13)`、`#10` 和 `(#1 #3)`。

`even?`、`odd?` 恰好接受一个 int；`zero?`、`positive?`、`negative?` 接受一个 int 或 float。
它们都返回布尔值，不隐式转换文本或布尔输入。浮点负零是零，既不是正数也不是负数。

| 调用 | 结果 |
| --- | --- |
| `(list/flat-map f xs)` | 拼接回调返回的列表，只展平一层 |
| `(list/sort-by key-fn xs)` | 按键稳定升序排序，每个元素只计算一次键 |
| `(list/group-by key-fn xs)` | 文本键到原始元素列表的映射 |
| `(list/take-while predicate xs)` | 首次假结果之前的前缀 |
| `(list/drop-while predicate xs)` | 从首次假结果开始的后缀 |
| `(list/partition size xs)` | 连续分组，每组最多 size 项，保留末尾不足一组的元素 |

变换从左到右访问元素，保留嵌套值。`flat-map` 要求回调返回列表。
排序键必须全部为文本，或者全部为有限数值；整数与浮点可混合，保留大整数比较精度。
键相等时保留输入顺序。`group-by` 要求文本键，保留分组首次出现顺序和组内元素顺序。
`take-while`、`drop-while` 要求布尔谓词，并在首次假结果后停止调用谓词。
空输入返回空列表或空映射。`partition` 的 size 必须为正整数，可以大于输入长度。

## 嵌套映射

| 调用 | 结果 |
| --- | --- |
| `(map/update m key f args...)` | 将旧值和额外参数传给 f，返回更新后的新映射 |
| `(map/get-in m path [default])` | 嵌套路径处的值，缺失时使用默认值或报错 |
| `(map/set-in m path value)` | 在路径处保存 value 后的新映射 |
| `(map/update-in m path f args...)` | 将嵌套旧值和额外参数传给 f，返回更新后的新映射 |

路径包含 1–128 个文本键，只遍历映射；已存在的中间值不是映射时会报错。
`set-in` 和 `update-in` 会创建缺失的中间映射。更新回调只调用一次，缺失的叶子值以 nil 传入，
需要时由回调显式处理。更新已有键保留原位置，新键追加到末尾。

`get-in` 将已有 nil 视为命中。显式默认值与 `map/get` 一样严格求值，命中时也会求值。
缺失的中间键可以使用默认值；中间值类型错误仍会报错。

```parsh
(let original {http {retries #2}})
(let updated (map/update-in $original [http retries] (function +) #1))
(dump [$original $updated])
(dump (map/get-in $updated [http retries]))
```

原映射保留 `#2`，更新后的映射包含 `#3`。

## 预置宏

每个会话启动时都提供以下普通宏，允许使用 `defmacro` 重新定义：

| 调用 | 行为 |
| --- | --- |
| `(when condition body...)` | 条件成功时执行主体 |
| `(unless condition body...)` | 条件失败时执行主体 |
| `(cond (condition body...) ... (else body...))` | 执行首个匹配分支；可选的 else 必须在最后 |
| `(-> value steps...)` | 将上一步表达式插入每步调用的第一个参数位置 |
| `(->> value steps...)` | 将上一步表达式插入每步调用的最后一个参数位置 |

条件遵循现有布尔值或 `CallResult` 规则。只执行选中的主体，返回主体最后一个值；
没有选中主体时返回 nil。`cond` 的每个分支都需要一个条件和至少一个主体表达式，
裸词 `else` 表示默认分支。穿线步骤可以是调用表单或可调用名称；初始表达式只求值一次，
没有步骤时返回初始值。

```parsh
(dump (cond ((< #3 #2) small) ((= #3 #3) equal) (else other)))
(dump (-> {http {retries #2}} (map/get-in [http retries]) (+ #1)))
(dump (->> [#1 #2 #3 #4] (list/filter even?) (list/map (partial + #10))))
```

结果依次为 `"equal"`、`#3` 和 `(#12 #14)`。宏不能通过 `function` 引用或 `apply` 调用。
自定义宏见[结构与宏](/parsh/zh-cn/docs/structs-and-macros/)。

## 表单、读取与求值

Form 是语法值，与运行时列表不同。读取器将 `[a b]` 表示为对 `list` 的调用，
实参为 `a` 和 `b`。

| 调用 | 结果 |
| --- | --- |
| `(form/kind form)` | `word`、`variable`、`text`、`int`、`float`、`bool`、`nil`、`list`、`interpolated` 或 `comment` |
| `(form/value form)` | 原子节点的值；裸词和变量名返回文本，列表与插值表单报错 |
| `(form/children form)` | 列表或插值表单的子表单列表 |
| `(form/list forms)` | 从运行时 Form 列表构造调用表单 |
| `(form/word name)` | 从一个裸词或标识符表单构造裸词表单 |
| `(form/variable name)` | 从合法名称或标识符表单构造变量引用 |
| `(form/from-value value)` | 为文本、数值、布尔、nil、列表和映射构造字面数据语法 |
| `(form/source form)` | 源码表示文本 |
| `(read text)` | 读取恰好一个表单，不执行，忽略注释 |
| `(eval form)` | 在调用方外围表达式的作用域中求值 |
| `(macroexpand-1 form)` | 展开最外层的一次宏调用，不执行展开结果 |
| `(gensym [prefix])` | 新的、具有独立身份的标识符表单；prefix 默认为 G |

`form/from-value` 保留字面数据，包括文本内的美元符号；函数、命令、bytes、结构体和 Form
不能作为字面数据输入。`form/list` 的每个子项都必须是 Form，构造深度最多为 256 层；
`form/from-value` 的数据嵌套深度最多为 128 层。

`read` 遇到空输入、多个表单或非法源码时抛出 kind 为 `read` 的可捕获错误。
源码包含需要在求值时解析的变量时，使用原样字符串：

```parsh
(let n #10)
(dump (eval (read '(+ $n #2)')))
(dump (macroexpand-1 (quote (when #true (+ #1 #2)))))
```

结果依次为 `#12` 和 `(if #true (do (+ #1 #2)))`。`eval` 接收 `Form`，`read` 将源码文本读为表单。
函数、外部命令、宏和特殊形式都按手写代码的规则求值。独立的顶层 `eval` 可以引入后续表达式可见的绑定，
求值错误指向 `eval` 调用处。 [调用与求值](/parsh/zh-cn/docs/syntax/#调用与求值)

`macroexpand-1` 执行最外层的宏体及其副作用，并以未求值的表单返回展开结果。
非宏表单原样返回。

`gensym` 的身份在复制和插入语法时保留。通过 `form/word` 与 `form/variable` 在绑定名称和引用之间
转换时也保留身份，普通文本名称无法重建它。前缀必须为合法变量名。
生成标识符显示为 `#<gensym:...>` 或 `#<gensym-var:...>`，这些显示文本不能重新读取。
`form/value` 只提供显示名称，保留身份时应继续传递 Form 本身。

```parsh
(defmacro twice (expression)
  (let temporary (gensym temporary))
  (let reference (form/variable $temporary))
  `(do (let ,$temporary ,$expression) (+ ,$reference ,$reference)))
(dump (twice (+ #1 #2)))
```

该宏只求值一次传入表达式，返回 `#6`。生成绑定不会捕获调用方变量；模板中的其他标识符仍遵循现有的非卫生宏规则。

## 加载脚本

`(load path)` 读取 UTF-8 源文件，完整解析后在调用方外围表达式的作用域中执行顶层表单。
返回最后一个值，空文件返回 nil。输出使用调用方的流，脚本参数保持继承；每次调用都会重新执行文件。

最外层相对路径基于当前工作目录。正在加载文件时，嵌套的相对加载基于该文件所在目录。
原生环境使用规范化后的实际路径检查循环加载。稍后从已保存函数发起加载时，使用当时的加载上下文或工作目录。
同时加载最多 32 个文件，每个源文件最多 8 MiB；循环加载和非 UTF-8 文件都会报错。

```parsh
(load examples/lib/greetings.parsh)
(dump (greet-library parsh))
```

原生 parsh 读取本地文件系统，浏览器会话读取虚拟文件。
自定义 `ExecutionHost` 可以实现 `read_file`，默认实现报告不支持加载；纯语法会话没有文件系统。
`load` 执行文件，用户模块声明仍属于独立的后续能力。

解析错误的 kind 为 `read`，文件访问和加载限制错误为 `load`，求值失败保留原有 kind。
错误消息包含加载的文件名与源码位置。语法错误会阻止该文件执行；运行失败保留已经执行的表单产生的效果。
可以使用 `try` 捕获失败，`return`、循环控制与 `exit` 保持通常含义。
