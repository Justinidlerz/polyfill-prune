# 开发与数据更新

## 构建

工具链：Rust >= 1.97.0、Node.js >= 22.12.0、pnpm 10.20.0。仓库开发使用的 napi CLI 与 Vite 可能要求更高的 Node 补丁版本，以依赖的 engines 为准。

```sh
pnpm install --frozen-lockfile
pnpm build
pnpm typecheck
pnpm check:rust
pnpm check:scripts
pnpm example
pnpm example:vite
```

`pnpm build` 先构建本机原生库，再构建核心 JS API 和 Vite 包。需要 C/C++ 链接工具；安装发布包只需要对应平台的预构建二进制。

| 目录                      | 内容                                      |
| ------------------------- | ----------------------------------------- |
| `native/src`              | Rust 分析、裁剪、映射、N-API 和数据生成器 |
| `native/data/compat.json` | 提交到仓库并嵌入原生库的兼容性数据        |
| `src`                     | JS API 和 TypeScript 类型                 |
| `packages/vite-plugin`    | 独立 Vite 包                              |
| `npm/*`                   | 平台包                                    |
| `examples`                | API、已 bundled 依赖和 Vite 示例          |
| `scripts`                 | 构建、版本同步和发布工具                  |

## 数据更新

```sh
pnpm data:sync
pnpm build
```

生成器使用锁文件中的 `@mdn/browser-compat-data` 与 `typescript`。升级相应开发依赖后重新生成，检查数据差异；核心运行和用户安装均不联网下载兼容性数据。

Oxc 解析 TypeScript 的 `lib.es*.d.ts`，提取全局声明、构造器/可调用接口、静态方法与 prototype 方法，随后关联 BCD 的 `javascript.builtins`。命名空间、继承关系、访问器、可选或计算属性未建立完整映射时不收录。

新增规则或参数同时更新 API、识别范围及插件文档。代码不自动提交或推送。

[返回首页](../README.md)
