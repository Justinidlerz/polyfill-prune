# 构建、打包与发布

采用 Changesets、napi-rs 五平台构建和 npm 平台可选依赖。根包与平台包固定同版本，Vite 包独立版本。

## 工作流

| 触发                               | 行为                                         |
| ---------------------------------- | -------------------------------------------- |
| PR                                 | 检查、准备 beta 版本、五平台构建、示例和打包 |
| main push                          | 检查、消费 Changesets、准备稳定版本并打包    |
| 手动运行且不填 run ID              | 只检查和打包                                 |
| 手动填写已完成 main Release run ID | 使用原归档恢复发布，不重新构建               |

发布需要待发布版本，并且 `NPM_RELEASE_USER` 与触发者匹配；PR 还需同仓库、非 draft 且作者匹配。npm 使用 Trusted Publishing/OIDC，每个平台包、核心包、Vite 包均需配置对应仓库与 `release.yml`。

发布脚本按依赖顺序发布，核对已存在版本的归档完整性。稳定版本发布成功后，工作流记录版本文件和 tags；本地构建及 Changesets 命令不会提交。

## 包检查

`pnpm pack:release` 要求 `dist/` 中恰好包含 `.github/native-targets.json` 的五个原生文件，核对原生版本、声明文件、入口、平台元数据及精确可选依赖版本，再生成 `artifacts/*.tgz` 和发布清单。

本机构建只生成本机二进制，无法代替五平台 CI。未配置 GitHub remote、仓库变量及 npm Trusted Publisher 前不会发布。

[返回首页](../README.md)
