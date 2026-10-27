# 为 Flash 贡献代码

感谢你的关注。Flash 目前由一人维护，下面的规则旨在让双方都能低成本完成评审。

## 开始之前

- 除小型修复外，请先创建 issue，确认方案后再开始编写。调度行为（FSRS、队列顺序、每日限制）以及涉及账户或令牌的改动会受到额外审查。
- 贡献代码需要遵守 [CLA.md](CLA.md) 中的贡献者许可协议。你第一次提交拉取请求时，机器人会提示你签署；只需完成一次。

## 构建与测试

```
cargo test --workspace
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
```

以上三项都必须通过，CI 会在每个拉取请求上运行它们。任何平台只需要 Rust 工具链（Windows 需要 rustup 提示安装的 MSVC 构建工具）；加密使用 ring 和 RustCrypto，不需要安装 OpenSSL、Perl 或 NASM。

目录结构概览：

- `crates/flash-core`：纯领域代码，不包含 IO。调度器参考向量位于
  `tests/fsrs_vectors.json`，不要随意重新生成。
- `crates/flash-store`：所有 SQL 语句。迁移文件只能追加：已经发布的迁移不可修改，只能添加下一个编号。
- `crates/flash-server`：HTTP 服务。处理器应保持精简；所有规则放在
  `service.rs` 或 `flows` 模块中，避免 Web UI、JSON API 和 MCP 工具出现行为偏差。模板使用 Askama，交互行为使用 htmx。

测试采用集成测试方式，并在内存数据库上运行；测试框架位于 `crates/flash-server/tests/it/`（`common::*`）。每个 crate 的集成测试都是 `tests/it` 测试二进制中的模块，因此新增测试文件时，需要在 `tests/it/main.rs` 中新增一行 `mod`。可以使用过滤器运行单个文件或测试：

```
cargo test -p flash-server --test it mcp_tests::
cargo test -p flash-server --test it mcp_tests::a_name
```

## 拉取请求

- 每个拉取请求只包含一项改动，并添加在没有该改动时会失败的测试。
- 在描述中说明行为变化；提交消息应说明原因，而不只是改了什么。
- 保持 diff 简洁，避免格式化噪声和无关重构。

本仓库是私有仓库的只读镜像，因此历史记录会以批次同步，而不是合并提交的形式到达。我们接收的拉取请求会在下一次同步中作为独立提交进入仓库。如果你的代码基本按原样合入，你是该提交的作者；如果维护者基于你的工作进行了重写，你是共同作者。如果后续工作又重写了你的改动，则你会成为同步主提交的共同作者。完成同步后，我们会关闭拉取请求，并附上对应提交的链接。

## 发布与更新日志

[CHANGELOG.md](CHANGELOG.md) 会记录 Flash 使用者能够感知的每项变更，并在正式发布前按时间倒序放在 `Unreleased` 标题下。改变行为的拉取请求应在同一次改动中补充对应条目；重构或测试无需添加。条目应说明用户看到的变化，而不是移动了哪个函数，并以关联 issue 和提出者结尾：想法使用 `(#12, suggested by @handle)`，缺陷使用 `(#12, reported by @handle)`。提出者的署名会保留在更新日志和发布说明中。

版本遵循语义化版本规范。发布版本是在本仓库创建的 `vX.Y.Z` 标签，并且必须与 `Cargo.toml` 中工作区版本一致；发布工作流会拒绝不一致的标签，使用更新日志对应章节作为发布说明，附带静态二进制文件并推送镜像。

## 报告安全问题

任何可能影响他人数据的问题都不要公开创建 issue。请按照 [SECURITY.md](SECURITY.md) 的说明，通过仓库的 Security 标签页私下报告，并留出几天等待回复。
