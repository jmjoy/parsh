---
title: "执行结果与后台任务"
description: "可调用命令、完整结果、$?、异常处理和原生后台任务。"
---

## 可调用命令

函数、普通内置与外部命令共用调用接口。`command` 创建外部程序引用，强制绕过同名函数：

```parsh
(let print (command printf))
($print "<%s>\n" "a b")
(apply $print "<%s>\n" [first second])
(apply + [#1 #2])
```

`apply` 接受函数值、命令引用或文本名称。文本名称可以是引号字符串，因此可以动态选择命名空间函数：

```parsh
(apply "str/join" [[a b] ","])
(apply "map/get" [{name parsh} name])
```

`function` 仍只取得用户函数和普通内置。`command?`、`callable?` 用于检查引用。
名称沿用普通查找规则，宏和 `if` 等特殊形式不能作为普通被调用对象。
实参从左到右求值一次，`apply` 不将值重新解析成源码。

普通调用保留原来的取值规则：`(let here (pwd))` 得到去掉尾部 LF 的 UTF-8 stdout 文本，
算术和普通函数返回语言值。函数以命令结束时可以返回捕获输出；显式返回类型值则保留该值。

## 完整执行结果

`capture-call` 的首参是被调用对象，后面直接铺平实参；它捕获 stdout/stderr 并返回 `CallResult`：

```parsh
(let result (capture-call printf "hello\n"))
(list (stdout-text $result) (exit-code $result) (signal $result))
(let sum (capture-call + #2 #3))
(CallResult.value $sum)
```

| 字段 | 含义 |
| --- | --- |
| `value` | 语言返回值；外部命令为 nil |
| `termination` | `(Exited code)`、`(Signaled signal core_dumped)` 或 nil |
| `stdout`、`stderr` | 捕获的 `bytes`；显式重定向或未捕获时为 nil |
| `error` | 错误摘要或 nil |
| `stages` | 管道阶段的 `StageResult`，包含 argv、终止信息和错误 |

这些是运行时结构体，提供 `CallResult.stdout`、`CallResult?`、`Exited.code` 等访问器和谓词；
其名称与构造器由运行时保留。`bytes?` 判断原始字节，`typeof` 返回 `bytes`，`eq` 按内容比较。
bytes 显示为 `<bytes N>`，不能隐式转成文本、命令参数或环境变量。

`stdout-text`、`stderr-text` 严格解码 UTF-8，保留换行。无效 UTF-8 会抛出携带原始结果的错误。
信号终止时 `exit-code` 为 nil，正常退出时 `signal` 为 nil，因此显式退出码 143 与 SIGTERM 终止不同。
原生 Linux 的 core dump 信息保存在 `Signaled.core_dumped`。

`capture-call` 将非零退出和信号终止作为数据返回，启动、实参和语言错误仍抛异常。
函数内部命令失败仍停止执行剩余函数体。`check` 在结果失败时抛出异常，成功时返回同一个结构体：

```parsh
(try
  (check (capture-call cat missing.txt))
  (catch err (echo "read failed: $(Error.message $err)")))
```

显式文件重定向优先于捕获。两路捕获合计上限 8 MiB，超限时停止执行，在异常结果中保留已有输出。
未捕获的前台命令仍实时输出。管道阶段仍限外部命令；捕获或后台执行整个管道时使用函数包装：

```parsh
(capture-call (fn () (| (printf "b\na\n") (sort))))
```

## 上一次执行结果：`$?`

`$?` 是会话只读变量，保存完整 `CallResult`，初始为 nil。`(get ?)` 读取相同值。
需要数值退出码时显式使用访问函数：

```parsh
(let here (pwd))
(let saved $?)
(list $here (exit-code $saved) (signal $saved))
(echo "exit=$(exit-code $?)")
```

外部命令、管道、用户函数及 `cd`、`export`、`type` 等命令内置发布结果，失败时先更新再抛异常。
纯数据内置、字段访问、变量读取和任务状态查询不覆盖结果。绑定、控制流与 `apply` 不额外覆盖被包裹调用的结果。
`capture-call`、`wait`、`check` 发布其处理的那份结果。

普通调用只保留取值时已经捕获的输出，不会额外缓存终端输出。在下一条命令前绑定 `$?` 可以保存快照。
`?` 不能用于绑定、赋值、导出或形参。双引号识别 `$?`，但不隐式将结构体转成文本；插值时读取字段。
单引号与 `\$` 保留字面美元符。

## 异常与清理

```parsh
(try
  (throw {reason expected})
  (catch err (echo "caught $(Error.kind $err)"))
  (finally (echo cleanup-finished)))
```

`Error` 暴露 `kind`、`message`、`code`、`offset`、`payload`、`result`。
抛出已有 Error 会重新传播该错误，其他值成为用户异常的 payload。
`try` 必须有主体和至少一种处理块，catch/finally 各至多一个、catch 在前，分别拥有独立词法作用域。
主体执行前先校验全部处理块的结构。

catch 捕获运行错误，不捕获 return、break、continue、exit 或取消。
finally 在正常完成、异常和语言跳转时执行，忽略普通返回值；新异常覆盖此前结果，取消除外。
finally 的当前函数上下文禁止 return/break/continue，调用的辅助函数仍可正常返回。
强制取消可以打断清理。

## 原生后台任务

```parsh
(let job (spawn printf "hello %s\n" world))
(job-status $job)
(let result (wait $job))
(stdout-text $result)
(check $result)
```

`spawn` 接受外部命令、用户函数和普通内置，采用铺平实参，返回属于当前会话的 `Job`。
Job 不是稍后自动改变字段的结果结构体。`JobStatus` 快照包含 `state`（running/completed）和
`result`（完成前为 nil）。`wait` 返回包含异步语言错误的完整结果；重复等待保留相同结果及返回闭包的身份。
`cancel` 请求取消并返回 nil。Job 只能在原会话使用。

每个任务拥有独立的 stdout/stderr 捕获，stdin 默认 EOF，显式重定向优先。
函数在线程内重建隔离解释器：复制词法作用域、递归闭包、cwd、定义和环境，后台修改不影响父 Shell。
结果可以包含闭包。外部命令启动失败由 spawn 抛出，函数运行中的错误保存在结果里。
后台 exit 只结束当前任务。

spawn 发布的启动结果在 `value` 中保存 Job，termination 为 nil。
后台自行完成只更新任务状态；父 Shell 执行 wait 后才更新自己的 `$?`，任务内部有独立的 `$?`。
会话退出或重置时取消并回收任务与子进程。解释器线程协作取消，清理窗口为 200 ms，之后强制取消后续求值并终止子进程；
阻塞的原生 I/O 可能延迟线程回收，线程本身不会被强杀。

后台功能要求原生 parsh。浏览器支持同步调用、结果、bytes、`$?` 和异常，后台接口明确报能力不支持。
首版没有字面 `&`、暂停恢复、fg/bg 或脱离会话长期运行的任务。

显式捕获最多嵌套 32 层；普通求值保留独立的深度限制。
