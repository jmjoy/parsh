---
title: "函数与控制流"
description: "词法函数、条件分支、顺序求值与循环。"
---

```parsh
(defn greet (name)
  (echo hello $name))

(if (test -d src)
    (greet developer)
    (echo missing))

(if (eq $EDITOR vim)
    (echo using vim))

(if (not (test -f config.txt))
    (echo configuration missing))

(do
  (let message "multiple expressions")
  (echo $message))
```

`if` 只对选中的分支求值。条件必须产生布尔值（来自 `eq`、数值比较或 `not`）或命令退出状态；
退出状态为零表示真。文本和数值不会隐式转换为布尔值。
`do` 在当前作用域中依次执行表达式。
`for` 对列表求值，并为每次迭代创建新的词法绑定。

`while` 在每轮开始前检查条件。两种循环都接受一个或多个循环体表达式，并在每轮创建
独立作用域：`let` 的绑定留在本轮中，`set` 可以更新外层绑定。

`and/or` 从左到右短路，统一返回布尔值；`(and)` 为真，`(or)` 为假。
它们沿用 `if` 的条件规则，可直接检查命令状态。

```parsh
(let count #0)
(while (< $count #5)
  (set count (+ $count #1))
  (if (= $count #2) (continue))
  (if (= $count #4) (break (list stopped $count)))
  (echo iteration $count))
```

`(continue)` 跳过本轮剩余表达式。`(break)` 结束最近一层循环；`(break 值)` 对值求值一次，
并将其作为循环返回值，支持数值、列表、映射、布尔值和 `nil`。没有显式指定退出值时，循环返回
最近一次完整迭代的结果；没有完整迭代则返回 `nil`。被跳过或提前结束的迭代不会覆盖此前结果。
上述示例输出 `iteration 1` 和 `iteration 3`，并返回 `("stopped" #4)`。

循环跳转不能跨越函数边界。在当前函数没有活动循环体时使用会报错。循环条件或列表表达式
在自身循环体之外执行，其中的跳转只作用于已有的外层循环。`while` 循环体中的命令失败
会立即停止执行，即使整个循环被用作条件也是如此。

函数名和参数声明使用裸名字；读取参数和局部变量的值时使用 `$name`。
函数捕获定义时的词法作用域，并返回最后一个表达式的值。
参数表达式在绑定形参之前求值。函数既可以返回命令输出，也可以返回数值、列表、映射和布尔值。
函数用作值时，会捕获整个函数体的标准输出：如果最后一个表达式产生命令退出状态，
则返回捕获的输出文本；否则直接返回最后一个表达式的值，此前捕获的输出不再显示。

## 函数值与参数

`fn` 创建匿名函数，`function` 显式取得命名函数或普通内置函数的值。
函数值可以保存在集合中、传参或返回，并支持表达式作为调用头：

```parsh
(let inc (fn (x) (+ $x #1)))
($inc #2)
((fn (x) (* $x #2)) #3)
(let add (function +))
(apply $add #1 [#2 #3])

(defn sum (first &optional (second #0) &rest more)
  (let result (+ $first $second))
  (for n $more (set result (+ $result $n)))
  (return $result))
(sum #1 #2 #3 #4)
```

参数顺序为必选参数、`&optional`、`&rest name`。可选参数可以只写名字，缺省为 nil；
`(name default)` 的默认表达式仅在缺参时、每次调用中求值，可以访问前面的参数和捕获环境。
可选参数按位置依次消费实参，剩余实参组成 rest 列表；列表实参保持为一个完整值。
`apply` 将最后一个列表展开为实参，并允许在列表前提供固定实参，所有值都不会再次解析或求值。

普通数据继续采用值语义；复制函数值保留相同的函数身份和捕获环境。
`eq` 按函数身份比较；函数引用保留取得时的定义，后续重定义不会改变已有引用。
闭包读取定义处的词法作用域，外层 `set` 的更新仍可见。函数值显示为 `<function name>`，
不能隐式转为文本、外部命令参数或环境变量。
`function` 不接受宏、外部命令或 `if`、`and`、`let` 等特殊形式。
`get`、`not`、数值与集合操作，以及 `glob`、`cd`、`type` 等普通内置支持函数引用。

`(return)` 提前返回 nil，`(return expr)` 求值一次并退出当前函数；可以穿过循环、
嵌套表达式和重定向，不会退出调用方函数。在函数外使用会报错。
命令表达式作为返回值时沿用普通值位置的输出捕获规则。
宏体也可用 return 提前返回表单；宏展开代码中的 return 则作用于调用方函数。

## 观察返回值

```parsh
(defn greet (who)
  (str "hello, " $who))
(greet parsh)
(if #true (list yes) (list no))
(for item (list first last) (str $item))
```

交互环境依次显示 `"hello, parsh"`、`("yes")` 和 `"last"`。循环返回最后一次循环体的结果，不会收集每次迭代的值。这组示例可以直接在浏览器终端中运行。
