# MCP 卡片内容与编辑

Flash 0.3.0 修改了 MCP 协议。若从早期版本升级，请刷新缓存的工具定义。
`update_card`、`delete_card` 以及使用普通 `front`/`back` 参数创建卡片均没有兼容别名。

## 牌组与子牌组

牌组通过名称寻址，名称也是路径：`Pharmacology::Cardio` 表示 `Pharmacology` 中的 `Cardio` 牌组，与 Anki 一致。所有接收牌组名称的工具都接收路径。`::` 两侧的空格和空路径段会被忽略。

如果路径中的上级牌组不存在，`create_deck` 和 `create_cards` 会在同一事务中创建它们。一次 `create_cards` 调用只填充一个牌组，因此向包含多个子牌组的树添加卡片时，每个子牌组需要调用一次。

一个牌组包含其下的所有牌组。在 `Pharmacology` 上调用 `start_study_session` 和 `list_cards` 也会覆盖 `Pharmacology::Cardio`，每张卡片仍会消耗其所属牌组的每日限制。`archive_deck` 和 `delete_deck` 会连同子牌组一起处理。`unarchive_deck` 会恢复它们以及目标牌组所在的上级牌组，确保活动牌组不会位于已归档牌组内。同理，已归档牌组中不能新建牌组，但仍可向已经存在的已归档牌组添加卡片。

`list_decks` 会按每个牌组后跟其子牌组的顺序返回结果。牌组的 `due` 和
`new` 已经包含子牌组数据，不要再次累加各行。

`rename_deck` 使用牌组当前完整名称作为 `deck`，使用新的完整名称作为 `new_name`。由于名称是路径，该操作可以重命名牌组、将牌组移动到另一个牌组中（例如从 `Cardio` 移动到 `Pharmacology::Cardio`，必要时创建 `Pharmacology`），或将牌组移到顶层。子牌组、卡片和复习历史都会随之移动。以下情况会拒绝操作：目标名称已被占用（两个牌组不会合并）、新名称位于牌组自身内部、子牌组完整名称超过 200 字节，或活动牌组会被移动到已归档牌组中。

## 目标日期

牌组可以设置目标日期，即用户需要掌握全部内容的日期，以账户日历中的 `YYYY-MM-DD` 表示。`create_deck` 通过 `goal_date` 设置目标日期；带牌组参数的 `set_limits` 可以设置、修改或使用空字符串清除目标日期。日期必须晚于今天且不超过五年。

日期尚未到来时，系统会按一定速度安排新卡，使所有新卡都能在最后几天之前出现；最后几天约占剩余时间的八分之一，通常为三到十四天，用于复习。该速度会替代牌组的新卡限制，并叠加在账户限制之上。如果某张卡在目标日期之后复习时，届时记住它的概率会低于 95%（比账户期望记忆率高 5 个百分点，最高 99%），系统会把这次复习提前安排到最后几天之一。包含在目标牌组中的子牌组会继承目标，除非它有自己的目标日期。

`list_decks` 和 `get_limits` 会返回牌组的 `goal`，包括 `date`、`days_left`、`new_per_day`、跨牌组及其子牌组统计的 `learned` 和 `total`、`on_track`，以及目标设置时计划中落后的新卡数量 `behind_by`（添加卡片会创建新计划，因此只计算跳过的天数）。从目标日期当天起，牌组按普通方式学习；日期过去后，`goal` 会返回 `passed: true`，直到清除目标或归档牌组。

## 阅读与学习

`list_cards` 会返回渲染后的 `front_html` 和 `back_html`、可编辑的 `source`（笔记 ID/类型、正反面 HTML、标签、同组卡片 ID）以及媒体元数据。纯文本会进行 HTML 转义。格式化内容是 Flash 保存的已清理内容，而不是未经清理的输入；source 字段会保留完形填空标记。

请使用学习工具进行测试：在请求答案之前，它们只返回渲染后的问题面。客户端自行决定如何显示或朗读内容；不要朗读原始标记或 URL，也不要泄露完形填空答案。

`list_tags` 会列出用户的标签及每个标签包含的卡片数量，可以统计所有牌组，也可以通过
`deck`（路径）限定到某个牌组及其子牌组。`start_study_session` 使用这些名称作为
`tags`：卡片必须同时包含所有标签（最多 10 个）才会加入会话；同时提供 `deck` 时会
限定该牌组，不再像旧版本那样被忽略。`tag` 仍然可用，并会作为额外的一个标签。

学习队列为空时，`note` 会说明原因：今日限制暂时阻止了卡片、已经完成全部复习，或
范围内根本没有卡片。没有任何卡片的账户会得到“尚无卡片”的提示和创建卡片的建议，
而不会被误报为已完成；此时 `list_decks` 也会返回相同的 `note`。

分页使用 `before_card_id`：当 `has_more` 为 true 时，继续传入返回的 `next_cursor`。
默认限制为 50，最大为 500，按 ID 从新到旧返回。单页还受大小限制（卡片 JSON 约 64 KB），
因此实际数量可能少于 `limit`：普通卡片约两百张，富文本卡片约几十张。
是否继续分页应始终依据 `has_more`，不要依据页面长度。`total_count` 描述当前匹配集合，
不是跨多次调用保持不变的快照。

媒体条目包含 `media_id`、`url`、`mime`、`filename` 和 `size`。使用
`Authorization: Bearer <connector token>` 请求绝对 URL。下载路由支持既有的范围请求行为，
并会检查所有权。它不会把凭据转发到外部存储，也不会授予账户 API 访问权限。在可编辑 HTML
中请保留稳定的 `/media/{id}` 引用。MCP 结果不会嵌入图片、音频或视频字节；无法请求带认证
引用的客户端不能仅凭链接查看这些媒体。

## 面板

支持渲染 MCP Apps 视图（`io.modelcontextprotocol/ui` 扩展）的主机会在对话中显示 Flash 面板。无论是否显示面板，模型读取到的内容都相同。

- 运行 `start_study_session` 时，面板显示卡片正面和“显示答案”按钮，下方提供“再来一次”“困难”“良好”“简单”四个评分按钮，并显示进度条（例如“2 / 10”表示已完成复习数与队列总数）。每次点击都会记录一次复习，与 `submit_review` 相同。
- 运行 `create_cards` 或 `update_cards` 时，面板会逐张显示写入的卡片及正反面，可通过箭头或滑动翻页。编辑卡片时会同时显示修改前后的内容。创建卡片后，可以在同一面板开始学习所属牌组；`restore_cards` 会在同一视图中显示被恢复的卡片及其版本。
- 学习队列结束时，面板会根据 `next_due`（`at_ms`、`in_days`、`cards`）显示下一批卡片的时间，并根据 `week` 绘制未来七天的学习安排。
  `panel_show_answer` 携带 `comes_back_in_ms`，评分按钮会显示各评分后卡片再次出现所需的时间。学习视图还携带下一张卡片的 `next_card`；每次评分后，面板会立即显示下一张卡片。单牌组的已创建卡片视图携带该牌组页面的 `deck_url`。`start_study_session` 也会向模型提供相同的 `next_due` 和文字说明 `note`。
  带有 `reminders_offer` 的视图会在下一次到期日期旁显示提醒按钮，并使用浏览器时区调用 `panel_reminders`；开放服务器不会主动发送提醒。受到每日新卡限制时，会提供“再学习 N 张”按钮（最多 10 张），调用 `boost_new_today` 后重新开始 `start_study_session`。学习结束视图通过 `site_url` 链接到统计页面。
  点击卡片正面会显示答案。旧版桥接协议通过 `setOpenInAppUrl` 告诉主机当前视图对应的页面，供主机的“打开”按钮使用。会话卡片菜单包含“编辑卡片”和“归档卡片”。“归档卡片”调用 `panel_archive`，将卡片移出学习队列但保留在牌组中，并保留计划和历史；标题会显示“卡片已归档”和“撤销”，撤销时使用 `archived: false`。主机不支持相关工具时，面板不会显示对应功能。
  “编辑卡片”是快速编辑功能：`panel_edit_source` 提供卡片背后笔记的两个字段（反向或完形填空卡片可能只显示其中一部分），包括已有图片；用户可以修改和删除文字，但不能添加图片或格式。`panel_save_edit` 保存修改（笔记类型和标签不变，不能删除当前正在学习的卡片），并返回当前卡片正面、背面和下一张卡片正面。字段下方的高级编辑链接会打开站点上的 `/cards/{id}` 页面，链接地址来自 `card_url`，不会携带登录状态。

  面板会在 `ui/initialize` 中向主机提供 `inline` 和 `fullscreen` 两种显示模式，只有在用户
  点击“展开”、主机标题栏提供 `fullscreen` 时，或卡片即使缩到最小字号也无法在内嵌卡片中
  完整显示而用户点击“查看全部”时，才会请求全屏（`ui/request-display-mode`，旧桥接协议
  使用 `requestDisplayMode`）。不提供 `fullscreen` 的主机会让卡片自动增长以适应内容。会在
  第二个窗口中打开全屏的主机（ChatGPT 即如此）会运行两份页面；每份页面都把会话最新状态
  保存到自身来源的 `localStorage`，并跟随另一份页面同步。

- 面板资源为 `ui://flash/panel-v<N>.html`（`text/html;profile=mcp-app`），也是
  `resources/list` 列出的资源。每次改动都必须递增版本，因为主机会按 URI 缓存视图。由于
  主机会保留之前收到的工具列表，所有旧 URI（最早可追溯到 `ui://flash/study-panel-v2.html`）
  仍会返回当前页面。CSP 只允许从本服务器加载媒体和站点字体，不会请求其他来源。
- 打开面板的工具（`start_study_session`、`create_cards`、`update_cards`、`restore_cards`）会在
  `_meta.ui.resourceUri` 中指定面板。它们的结果会在 `_meta["flash/panel"]` 中携带面板视图，
  主机会从模型上下文中保留该视图；`view` 的值为 `study` 或 `cards`。声明支持扩展的主机
  不一定会实际绘制该面板，因此模型仍会请求第一张卡片；在显示面板的主机中，问题也会
  同时出现在聊天消息里。
- 面板自己的工具 `panel_show_answer`、`panel_grade`、`panel_cards`（用于分页查看下一批卡片，
  每次最多 20 张）、`panel_reminders`、`panel_edit_source`、`panel_save_edit` 和
  `panel_archive` 仅对应用可见（`_meta.ui.visibility: ["app"]`）。
  `start_study_session`、`end_session` 和 `boost_new_today` 对应用和模型都可见；其他工具（包括
  `create_cards`、`update_cards` 和 `restore_cards`）仅对模型可见，因此面板无法直接调用。
  如果模型已经在聊天中评分，`panel_grade` 不会重复记录；它会返回当前卡片并设置
  `recorded: false`。页面第一次评分会携带 `ui_locale`，表示主机告知面板的用户阅读语言
  （MCP Apps 的 `locale` 或 ChatGPT 的 `window.openai.locale`）。
- 工具调用中的 `_meta["openai/locale"]`（ChatGPT；旧客户端发送 `webplus/i18n`）会作为账户
  语言保存，面板的 `ui_locale` 也一样：每个账户每天最多更新一次，或在语言发生变化时更新。
  在用户尚未主动选择语言前，账户会采用其浏览器、助手或手机最近报告的语言。
- 面板不持有凭据，所有调用都通过主机完成。面板副本中的卡片媒体使用签名链接
  `/mcp-media/{id}/s?e=…&s=…`，有效期一天，签名密钥由服务器启动时生成。模型读取到的内容
  仍保留普通的 `/media/{id}` 引用。

## 创建与更新

`create_cards` 接受 1-500 个源对象，并在需要时创建指定牌组。字段包括 `front_html`、
`back_html`、可选的 `tags`，以及可选的 `note_type`：`basic`（默认）、`basic_reversed`、
`basic_typed` 或 `cloze`。生成的卡片数量可能多于源对象数量。

卡片中的本地媒体应使用稳定的 `/media/{id}` 引用。MCP 创建时发现的 HTTP(S)
图片会先随卡片保存，并由后台任务异步下载、校验后转成本地媒体；下载成功后替换为
`/media/{id}`。临时网络错误会自动重试，连续三次失败后只移除对应图片，不影响卡片其余文字。

```json
{"deck":"Pharmacology","cards":[{"front_html":"<b>Drug A</b>: mechanism?","back_html":"Answer"}]}
```

卡片字段的 schema 描述了如何为任意主题编写值得学习的卡片：
- 正面应能独立作答，并且只承载一个要点；选择题应包含选项和案例所需的上下文。列表和多部分规则应拆成多张卡片，或使用完形填空。
- 背面应是尽可能简短的准确答案，必要的补充信息放在答案之后。
- 正反面都保留原材料中的术语和数字；纯文本无法表达的内容用 HTML 编写（例如 `10<sup>5</sup>`），并让每段文本保持单一文字体系。

每张卡片还会说明正反面应如何朗读以及使用哪种语言：`front_speech` 和 `back_speech` 是跟随对应文本的语音片段列表。片段可以是使用 ISO 639-1 语言代码（在口音重要时附带地区）的词语（`{"lang":"es-MX","text":"¿Dónde está?"}`），也可以是使用听者自身语言朗读的提示（公式过长时使用 `{"say":"equation"}`，无法朗读的内容使用 `{"say":"look"}`）。不会朗读的内容（引用、文字旁的转写、表情符号、图片）会从片段中排除；拼写存在歧义时会按读音写出（例如日语汉字使用假名）。Schema 将这两个字段标记为必填，便于助手与卡片正文一起生成；即使缺少它们，卡片仍会保存。`create_cards` 和 `create_deck` 可通过可选的 `languages` 列表（`["ja","en"]`）声明牌组语言。

```json
{"deck":"Japanese","languages":["ja","en"],"cards":[{"note_type":"basic_reversed",
 "front_html":"How do you say cat in Japanese?","back_html":"猫（ねこ）",
 "front_speech":[{"lang":"en","text":"How do you say cat in Japanese?"}],
 "back_speech":[{"lang":"ja","text":"ねこ"}]}]}
```

`update_cards` 接受 `cards` 数组。每个条目通过 `card_id` 标识卡片，并只提供需要修改的源字段：
`front_html`、`back_html` 或 `tags`。省略的字段会保留；空的标签数组会清除标签。显式传入的空 HTML 仍需通过常规卡片类型校验。某一面文字发生变化时会清除其语音片段，除非编辑同时发送新的 `front_speech` 或 `back_speech`；未修改的另一面会保留原有片段。

```json
{"cards":[{"card_id":42,"back_html":"<b>Updated answer</b>"}]}
```

对于共享源，请在更新中原样复制其 `sibling_card_ids`，也就是该源生成的所有卡片（包括正在编辑的卡片）。每个源提交一次编辑，不要按每个兄弟卡片重复提交。如果 ID 不匹配，服务端会返回源实际拥有的 ID；同一源同时收到包含两个卡片 ID 的编辑也会被拒绝。所有兄弟卡片会原地更新，并保留 ID 和历史记录。笔记类型不会改变。新增的完形填空槽位可以创建卡片；删除已有槽位会被拒绝，必须先明确确认删除。结构发生变化后请重新读取。对于丢失富文本源或无法无损重建的旧导入数据，卡片仍可读取；源的 `read_only_reason` 会说明限制，可能造成内容丢失的重写会以 `source_unavailable` 拒绝。

批量请求会校验所有权、源成员关系、媒体和大小限制，并以原子方式提交。发生错误时整批请求都不会改变。重复目标会被拒绝。请求体最大仍为 1 MiB，因此包含大量 HTML 时可能需要少于 500 个条目。写入结果会返回源/卡片 ID、实际创建/更新数量以及牌组最终统计。`update_cards` 还会返回 `revision_ids`，按 `source_ids` 的每个条目对应一个值：表示该编辑保留的源的上一版本；如果编辑没有实际变化（文本、类型和标签集合都相同），则返回 `null`。

## 旧版本与恢复

通过 `update_cards`、Web 编辑器或应用对卡片源进行的每次编辑，都会在同一事务中把编辑前的源（笔记类型、两个字段和标签）保存为版本。尚无笔记的卡片（旧卡片或导入卡片）第一次编辑时，也会以相同方式保存卡片自身文本。每个源最多保留最新 20 个版本，且版本不会超过 90 天；更早的版本会在下一次编辑时清理。删除卡片所属牌组或账户时，这些版本也会被删除。

`restore_cards` 接受 `list_cards` 返回的 `card_id`。单独调用时，它会按从新到旧列出卡片的历史版本，每个版本包含 `revision_id`、`replaced_at_ms`、`replaced_by`（产生替换编辑的来源：`web`、`mcp`、`api`、`restore`，或下游扩展自定义的值）、笔记类型、简短的纯文本 `front_preview` 和 `back_preview`，以及 `tags`。请先向用户确认，再使用该 `revision_id` 再次调用：

```json
{"card_id":42,"revision_id":7}
```

恢复会沿用编辑的相同路径：源和其兄弟卡片会原地重写，并保留 ID、计划和复习历史；被替换的文本也会以 `restore` 产生一个新版本，因此恢复操作本身也可以撤销。返回字段与 `update_cards` 相同。其他卡片或其他账户的源版本不会被找到；会删除卡片的恢复操作会以 `implicit_card_removal` 拒绝（请先显式删除），会新增卡片的恢复操作则必须满足服务器设置的卡片数量限制。

## 删除

使用明确的 `card_ids` 调用 `delete_cards`。第一次响应会返回准确的 ID/数量、`expires_ms` 和 `confirm_token`，不会删除任何内容。向用户说明选择范围并请求一次确认；得到批准后，使用相同 ID 和令牌再次调用。删除单张卡片也按包含一个条目的批量操作处理。

`delete_deck` 对牌组名称使用相同协议，并会删除该牌组及其下级牌组。预览和最终结果都会包含 `decks_affected`，按名称列出目标牌组及其子牌组，并列出这些牌组中的卡片。请求确认时请说清所有受影响的牌组。之前已删除的卡片会随所属牌组删除，但不会重复计数。如果预览后有卡片加入目标牌组，或新增、移入、移出子牌组，请重新请求确认。原始目标被永久删除后，所有待处理的批准都会失效，即使之后有替代对象获得相同的数据库 ID。

确认有效期为十分钟，并绑定到账户和 OAuth 客户端；重新连接、刷新令牌或重启服务都不会使其提前失效。错误会区分 `confirmation_invalid`、`confirmation_expired`、`confirmation_target_mismatch` 和 `confirmation_target_changed`。每条错误消息都会以错误代码开头（错误的 `data.code` 中也有该代码），并说明调用没有删除任何内容以及下一步操作。失败的事务不会消耗令牌；已完成操作的重试会返回原始结果，回执至少在过期后保留 24 小时。
