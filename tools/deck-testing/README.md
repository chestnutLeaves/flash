# 牌组测试工具集

这些 Python 工具（需要执行 `pip install zstandard`）用于使用真实牌组和恶意构造牌组测试 Anki 导入流程。所有工具都读取并生成标准 `.apkg` 文件。

- **apkg_tool.py `<src.apkg>` `[media_budget_mb]`** 校验结构（条目清单、每个媒体文件的魔数检查，禁止可执行文件和脚本），并写出 `<src>-nomedia.apkg`（仅集合）；指定预算时还会写出 `<src>-somemedia.apkg`（保留媒体直到预算用尽，并重写清单）。
- **apkg_subset.py `<src.apkg>` `<out.apkg>` `<media_mb>` `<note_cap>`** 截取完整加载的子集：保留前 N 条引用媒体的笔记及其全部媒体，删除其他内容并清理集合。
- **torture_deck.py `<out.apkg>`** 构建现代格式（`.anki21b`，zstd + protobuf）的压力测试牌组，覆盖所有导入功能：完形填空（提示、嵌套、相同索引、12 索引、格式错误）、`{{type:}}`、MathJax 和旧版 LaTeX、真实音频/图片媒体（从指定的大型牌组中提取）、暂停/隐藏卡片、带垃圾条目的真实复习日志、FSRS-6 牌组选项、反向卡片对、XSS 探测、Unicode/RTL/表情符号和超大字段。

请只使用你有权使用的牌组；本目录不会提交任何牌组。生成的产物位于 `target/deck-tests/`（已加入 gitignore）。
