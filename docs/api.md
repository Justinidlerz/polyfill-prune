# 核心 API

## `prune(code, options): Promise<PruneResult>`

解析、作用域分析、兼容性判断、文本裁剪和 sourcemap 生成均在 Rust 工作线程执行，不执行输入代码。返回新代码，不修改磁盘文件。

| 参数        | 类型       | 默认值     | 说明                                                           |
| ----------- | ---------- | ---------- | -------------------------------------------------------------- |
| `code`      | `string`   | 必填       | JavaScript、TypeScript 或 JSX 源码                             |
| `targets`   | `string[]` | 必填       | 每个运行时的最低版本，例如 `chrome100`、`safari15.4`、`node22` |
| `filename`  | `string`   | `input.js` | 决定解析模式及 sourcemap 源文件名                              |
| `keep`      | `string[]` | `[]`       | 保留指定 API 及子路径，例如 `Promise` 或 `Array.prototype.at`  |
| `sourceMap` | `boolean`  | `true`     | 为发生裁剪的文件生成 sourcemap                                 |

支持 `chrome`、`edge`、`firefox`、`safari`、`ios`、`opera`、`node`、`deno` 加显式版本。重复运行时取最低版本。只有全部目标的数据都明确支持时才裁剪。

`es2015`、`esnext` 等语法目标不表示原生 API 支持，混合配置时忽略；只有语法目标时拒绝调用。Browserslist 查询和 `baseline-widely-available` 等别名由调用方先展开，Vite 插件使用 Vite 已解析的目标。

| 返回字段      | 说明                                                |
| ------------- | --------------------------------------------------- |
| `code`        | 裁剪后代码；未命中时为原始字符串                    |
| `changed`     | 是否应用了裁剪                                      |
| `map`         | Source Map v3 对象；未改变或关闭映射时为 `null`     |
| `removals`    | 每条记录包含 `start`、`end`、`kind`、`features`     |
| `diagnostics` | 解析错误、语义错误或整文件保留原因；无问题时为 `[]` |

移除记录的 `[start, end)` 是输入代码的 UTF-8 字节范围，描述被替换的完整表达式或语句。嵌套记录可能重叠，不能直接相加计算删除体积。`kind` 为 `if`、`conditional`、`logical`。Sourcemap 的列使用 UTF-16，映射到本次调用的输入；构建工具负责与上游映射合并。

无效配置和内部转换错误导致 Promise reject。输入语法或语义不完整时原样返回，并提供 diagnostics。

## `version()`

同步返回 `{ core, parser, bcd, typescript, features }`，分别为核心版本、Oxc 版本、MDN BCD 版本、类型数据来源版本和收录 API 数量。

[返回首页](../README.md)
