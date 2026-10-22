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

`if` 只对选中的分支求值。条件必须产生布尔值（来自 `eq`、`=` 或 `not`）或命令退出状态；
退出状态为零表示真。文本和数值不会隐式转换为布尔值。
`do` 在当前作用域中依次执行表达式。
`for` 对列表求值，并为每次迭代创建新的词法绑定。

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
