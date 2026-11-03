# vite-plugin-polyfill-prune

Vite 6–8 的 polyfill 裁剪封装。依赖 `polyfill-prune` 的 Rust API，不在插件中实现兼容性或 AST 规则。需要 Node.js >= 22.12.0，并满足所用 Vite 版本的 Node 要求。

## 接入

发布后通过 `pnpm add -D vite-plugin-polyfill-prune` 安装。仓库开发运行 `pnpm install && pnpm build`，再运行 `pnpm example:vite`。

```ts
import { defineConfig } from 'vite';
import polyfillPrune from 'vite-plugin-polyfill-prune';

export default defineConfig({
  build: { target: ['chrome100', 'safari15.4'] },
  plugins: [polyfillPrune()],
});
```

## 配置

| 参数                        | 默认值                  | 说明                                                         |
| --------------------------- | ----------------------- | ------------------------------------------------------------ |
| `targets`                   | 已解析的 `build.target` | 显式运行时目标数组；可覆盖仅有 ES 语法的构建目标             |
| `keep`                      | `[]`                    | 保留 API 及其子路径                                          |
| `sourceMap`                 | `true`                  | 为转换生成映射，交由 Vite 合并                               |
| `filter(id)`                | JS/TS 模块扩展名        | 开发阶段接收模块 ID，生产阶段接收 chunk 文件名               |
| `optimizeDeps`              | `true`                  | 在开发依赖预构建中调用核心 API                               |
| `onResult(result, context)` | 无                      | 接收每次分析结果；context 包含 filename 和 stage，可异步执行 |

`stage` 为 `dev`、`dependency` 或 `build`。虚拟模块、`?raw` 和 `?url` 请求跳过。若 filter 捕获可变外部状态，修改后通过 Vite `--force` 更新依赖缓存。

## 执行阶段

| 场景              | 接入点               | 输出                                                    |
| ----------------- | -------------------- | ------------------------------------------------------- |
| 开发源码          | `transform`，post    | 请求与 HMR 时返回裁剪代码                               |
| Vite 6/7 依赖优化 | esbuild `onLoad`     | 对 JS 输入执行裁剪，再交给 esbuild                      |
| Vite 8 依赖优化   | Rolldown `transform` | 原生 API 裁剪与映射链                                   |
| 生产构建          | `renderChunk`，pre   | 对已打包 chunk 裁剪，后续压缩与 hash 计算使用修改后内容 |

生产源码阶段不裁剪。输出阶段的其他插件如果在本插件之后继续注入 polyfill，需要调整它们的 hook 顺序。构建 chunk 的文件过滤不等于源模块过滤。

只处理 `client` environment；SSR、worker 和其他 environment 跳过。开发依赖缓存将目标、keep、filter 文本和核心/数据版本纳入插件标识。Vite 6/7 自定义 esbuild loader 若先接管文件，本插件的 onLoad 无法再次处理该文件。

只支持核心库能够证明的分支，不保证删除任意已 bundled 的 core-js 或自定义安装器。纯 `esnext`/`es2015` 目标必须补充 `targets`。
