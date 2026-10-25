---
title: "结构与宏"
description: "命名记录、被引用的表单，以及用户自定义命令。"
---

有两种形式可以扩展语言本身：`defstruct` 声明记录类型，`defmacro` 定义一种命令，
它的参数以“被引用的表单”传入。

## 结构体

`(defstruct Name field...)` 至少需要一个字段，并生成三类命令：

| 名字 | 含义 |
| --- | --- |
| `Name` | 构造器：按定义顺序每个字段一个参数 |
| `Name.field` | 访问器：从该结构体的值里读取一个字段 |
| `Name?` | 判定：值为该结构体时为真 |

```parsh
(defstruct Point x y)
(let point (Point 3 4))
(echo (Point.x $point) (Point.y $point))
(if (Point? $point) (echo a point))
```

结构体是普通值：可以放进列表和映射、传给函数，也可以被字符串插值捕获。
它按字段顺序显示，例如 `(Point 3 4)`；字段是文本时保留引号，显示为 `(Point "3" "4")`。
它按名字与字段比较，并遵循通常的按值拷贝语义。
结构体不可变，修改字段意味着重新构造一个值。

访问器从值本身读取字段，而不是查当前定义，因此重定义结构体不会让已有的值失效：

```parsh
(defstruct Point x y)
(let old (Point 1 2))
(defstruct Point x)
(Point.y $old)
```

`type` 可以查看这些成员，不会调用它们：

```parsh
(defstruct Point x y)
(type Point)
(type Point.x)
(type -t Point)
```

名字不能与内置形式、函数或宏冲突；构造器、访问器和判定名都属于该结构体。

## 宏

`(defmacro Name (parameters...) body...)` 定义一种命令，它的参数不会被求值：
每个参数绑定到对应实参的被引用表单，宏体执行后返回一个表单，再由 Shell 在调用方的作用域里求值。

反引号开始一个模板，`,` 把求值后的表达式插入模板，`,@` 把一个列表表单摊平到外层调用里：

```parsh
(defmacro unless (condition body)
  `(if (not ,$condition) ,$body))

(unless (test -f config.txt)
  (echo "configuration missing"))
```

宏参数和普通变量一样用 `$name` 读取。由于返回的表单在调用点求值，
模板中出现的变量指向调用方的绑定：

```parsh
(defmacro greet (who)
  `(echo hello ,$who))
(let who parsh)
(greet $who)
```

`quote` 构造一个不求值的表单。表单是普通值：`eq` 按语法比较，交互提示符按源码显示。

```parsh
(quote (a b))
(eq (quote (a b)) (quote (a   b)))
(defmacro begin (body) `(do ,@$body))
(begin ((echo one) (echo two)))
```

unquote 接受被引用的表单或标量值；splicing 接受被引用的列表表单，或者由被引用表单组成的列表。

## 限制

- 宏参数与 `defn` 一样是固定个数，没有剩余参数。
- 宏和结构体是全局定义，即使写在函数体内也一样。
- 没有卫生性（hygiene）：模板会把表单原样插入调用方作用域。
- 宏和结构体成员不能作为管道阶段。
- 嵌套的 quasiquote 会原样拷贝，不会按层级展开。
- 结构体没有字段修改器，也不支持动态字段名。
