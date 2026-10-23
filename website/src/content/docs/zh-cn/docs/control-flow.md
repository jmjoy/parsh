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

```parsh
(let count 0)
(while (< $count 5)
  (set count (+ $count 1))
  (if (= $count 2) (continue))
  (if (= $count 4) (break (list stopped $count)))
  (echo iteration $count))
```

`(continue)` 跳过本轮剩余表达式。`(break)` 结束最近一层循环；`(break 值)` 对值求值一次，
并将其作为循环返回值，支持数值、列表、布尔值和 `nil`。没有显式指定退出值时，循环返回
最近一次完整迭代的结果；没有完整迭代则返回 `nil`。被跳过或提前结束的迭代不会覆盖此前结果。
上述示例输出 `iteration 1` 和 `iteration 3`，并返回 `("stopped" 4)`。

循环跳转不能跨越函数边界。在当前函数没有活动循环体时使用会报错。循环条件或列表表达式
在自身循环体之外执行，其中的跳转只作用于已有的外层循环。`while` 循环体中的命令失败
会立即停止执行，即使整个循环被用作条件也是如此。

函数名和参数声明使用裸名字；读取参数和局部变量的值时使用 `$name`。
函数捕获定义时的词法作用域，并返回最后一个表达式的值。
参数表达式在绑定形参之前求值。函数既可以返回命令输出，也可以返回数值、列表和布尔值。
函数用作值时，会捕获整个函数体的标准输出：如果最后一个表达式产生命令退出状态，
则返回捕获的输出文本；否则直接返回最后一个表达式的值，此前捕获的输出不再显示。

## 观察返回值

```parsh
(defn greet (who)
  (str "hello, " $who))
(greet parsh)
(if (eq a a) (list yes) (list no))
(for item (list first last) (str $item))
```

交互环境依次显示 `"hello, parsh"`、`("yes")` 和 `"last"`。循环返回最后一次循环体的结果，不会收集每次迭代的值。这组示例可以直接在浏览器终端中运行。
