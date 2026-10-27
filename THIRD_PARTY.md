# 第三方声明

Flash 使用 GNU Affero 通用公共许可证 v3.0（`LICENSE`）授权，并随附以下第三方作品。

## 嵌入 `flash-server` 二进制文件的资源

| 资源 | 版本 | 许可证 | 文本文件 |
|---|---|---|---|
| [htmx](https://htmx.org) | 2.0.7 | Zero-Clause BSD | `crates/flash-server/static/LICENSES/htmx-2.0.7-0BSD.txt` |
| [KaTeX](https://katex.org) (JS, CSS, fonts) | 0.16.22 | MIT | `crates/flash-server/static/LICENSES/katex-0.16.22-MIT.txt` |
| [Geist](https://vercel.com/font) and Geist Mono (woff2, subset in two parts per face by `tools/fonts/geist_web.py`; Geist SemiBold is also inside the README's `screenshots/logo.svg`) | 1.800 | SIL Open Font License 1.1 | `crates/flash-server/static/LICENSES/geist-OFL-1.1.txt` |

## Rust 依赖

`Cargo.lock` 中的每个 crate 都带有自己的许可证，且必须属于
`deny.toml` 列出的许可证之一（Apache-2.0、MIT、MIT-0、BSD-2-Clause、BSD-3-Clause、0BSD、
ISC、MPL-2.0、Unicode-3.0、Zlib、BSL-1.0、CC0-1.0、CDLA-Permissive-2.0、
Unlicense、bzip2-1.0.6）；`cargo deny` 会拒绝其他许可证。可使用
[cargo-license](https://github.com/onur/cargo-license) 列出许可证：

```
cargo install cargo-license
cargo license --workspace
```

FSRS 调度器使用 Open Spaced Repetition 项目的
[`fsrs`](https://crates.io/crates/fsrs) crate（BSD-3-Clause）；
`crates/flash-core/tests/` 下的参考向量由
[py-fsrs](https://github.com/open-spaced-repetition/py-fsrs) 生成。
