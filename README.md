# polyfill-prune

按目标运行时删除 JavaScript 中不再需要的 polyfill 分支。核心使用 Rust + Oxc，通过 N-API 提供可直接调用的 JS API；Vite 插件是仓库中的独立包。

| 包                           | 用途                                   |
| ---------------------------- | -------------------------------------- |
| `polyfill-prune`             | 异步裁剪 API、移除记录、sourcemap      |
| `vite-plugin-polyfill-prune` | 开发转换、依赖预构建、生产 chunk 裁剪  |
| `polyfill-prune-<platform>`  | 五个平台的原生二进制，由核心包自动选择 |

## 使用

Node.js >= 22.12.0。当前仓库版本未发布，先[本地构建](docs/development.md)。发布后通过 `pnpm add -D polyfill-prune` 安装。

```ts
import { prune } from 'polyfill-prune';

const result = await prune(code, {
  targets: ['chrome100', 'safari15.4'],
  filename: 'vendor.js',
});
// result.code / result.map / result.removals / result.diagnostics
```

```ts
import { defineConfig } from 'vite';
import polyfillPrune from 'vite-plugin-polyfill-prune';

export default defineConfig({
  build: { target: ['chrome100', 'safari15.4'] },
  plugins: [polyfillPrune()],
});
```

例如 `Object.assign || function fallback() { /* … */ }` 在所有目标均支持 `Object.assign` 时保留原生实现，删除 fallback。`if (!Array.prototype.at) { /* install */ }` 在目标均支持时删除安装分支。

## 范围

兼容性来自固定版本的 MDN Browser Compat Data，API 类型和原型位置来自 TypeScript 标准库。支持 `if`、三元表达式及 `&&` / `||` / `??` 中的原生 API 存在性和 `typeof` 检查，也可处理已 bundled 或压缩后仍保留这些结构的代码。

这是对可证明分支的裁剪。未提供通用 core-js 安装器识别、任意 polyfill 指纹匹配、运行时 bug 检测替换或整包卸载。未知结构、局部遮蔽、动态作用域和存在冲突写入的 API 保留。详见[识别范围与限制](docs/recognition.md)。

## 文档

- [核心 API](docs/api.md)
- [Vite 插件](packages/vite-plugin/README.md)
- [识别范围与限制](docs/recognition.md)
- [开发与数据更新](docs/development.md)
- [构建、打包与发布](docs/releasing.md)
- [第三方数据许可](licenses/README.md)
