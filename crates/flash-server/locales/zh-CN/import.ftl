### The open core's words in English: importing cards from a CSV file or
### an Anki package (.apkg). Every message has a note above it: where it
### shows, who reads it, and what each variable holds. Messages named by
### the store's parsers (`ImportNote`) are here too.

## The import page

# Browser tab title of the import page.
import-title = 导入 - Flash
# Heading of the import page.
import-heading = 导入
# What files the page takes. The code parts are a CSV's header row and
# Anki's file extension, shown as code; keep them as they are.
import-intro-html =
    带有 <code>front,back,tags,deck</code> 标题行的 CSV，或 Anki <code>.apkg</code> 导出文件。
    卡片会作为新卡导入，除非你同时导入 Anki 的学习进度。
# Label over the file picker.
import-file = 文件
# Button that opens the file picker.
import-choose = 选择文件
# Beside that button before a file is picked; a file may also be dropped
# on the form.
import-no-file-chosen = 未选择文件，或在此处放置一个文件
# Label over the field naming the deck the cards go into.
import-into = 导入到牌组
# Example in that empty field: a deck name.
import-deck-placeholder = 例如药理
# Shown on the button while the file uploads and is read.
import-reading = 正在上传并读取牌组...
# Button that uploads the file for a preview.
import-preview-button = 预览
# Heading of the preview.
import-preview-heading = 预览
# How many cards will import, and from which file. $count is the cards,
# $name the file's name (shown as code), $size its size (import-size-*).
import-ready-html =
    { $count ->
       *[other] <span class="mono">{ $name }</span> 中有 <strong>{ $count } 张卡片</strong> 可导入 <span class="muted">({ $size })</span>。
    }
# Badge: rows of the file that were skipped. $count is how many.
import-skipped =
    { $count ->
       *[other] 跳过 { $count } 行
    }
# Label over the list of the decks the file holds.
import-decks-in-file = 该文件中的牌组
# How many cards a deck in the file holds. $count is the number of
# cards.
import-deck-cards =
    { $count ->
       *[other] { $count } 张卡片
    }
# After the list when it was cut short. $count is how many more decks.
import-more = +{ $count }更多
# What happens to the file's decks. "Parent::Child" is how a deck path
# is written (shown as code; keep it).
import-keep-hint-html = 每个牌组都被保留，<code> Parent::Child</code>成为子牌组。您已经拥有的同名牌组将获得这些牌。
# Label over the field naming a deck to hold all the file's decks.
import-put-under = 放在下面（可选）
# Placeholder of that field when it is empty: the decks stay at the top.
import-top-level = 顶层
# Hint under that field.
import-put-under-hint = 命名一个牌组以将它们全部作为子牌组，或将其留空。
# Placeholder of the deck name field for a file that names no deck.
import-name-deck = 为这牌组命名
# Disclosure with the import's extra choices.
import-advanced = 高级
# Choice: keep the cards' colours. $count is how many cards have them.
import-keep-colors =
    { $count ->
       *[other] 保留牌组颜色，并匹配 Flash 配色（{ $count } 张带颜色的卡片）。
    }
# Choice: drop the colours.
import-clean-look = 外观正常，导入内容没有颜色。
# Choice: keep the Anki review history. $count is how many reviews.
import-bring-progress =
    { $count ->
       *[other]
            导入我的学习进度（找到 { $count } 次复习）：
            卡片保留原有计划，不会作为新卡开始；每次复习都会计入统计，包括连续学习记录。
    }
# Choice: use the package's Anki scheduling settings. $settings is a
# short list of them (import-settings-*).
import-adopt-settings = 采用该牌组的 Anki FSRS 设置 ({ $settings })。
# What adopting them does.
import-adopt-hint = 替换您当前的计划设置。
# Shown on the button while the cards and media import.
import-importing = 导入卡片和媒体...
# Button that imports. $count is how many cards.
import-submit =
    { $count ->
       *[other] 导入 { $count } 张卡片
    }
# Badge after an import.
import-imported = 已导入
# Line after an import. $count is how many cards were added.
import-added =
    { $count ->
       *[other] 已添加 { $count } 张卡片。
    }

## A file's size: $n is the number, already written as the reader writes it

# Megabytes. $n is the size in megabytes.
import-size-mb = { $n } MB
# Kilobytes. $n is the size in kilobytes.
import-size-kb = { $n } KB
# Bytes. $n is the size in bytes.
import-size-b = { $n } B

## The package's Anki settings, listed in brief (comma-separated)

# The retention it aims for. $pct is a percentage.
import-settings-retention = 保留率 { $pct }
# Scheduling parameters tuned to the person. $count is how many values.
import-settings-params = 调整后的 FSRS 参数（{ $count } 个值）
# New cards per day. $count is the number.
import-settings-new =
    { $count ->
       *[other] 每天新增 { $count } 张卡片
    }

## What the preview notes about the file

# The file's decks go into one. $count is how many decks, $decks their
# names, comma-separated.
import-merge = 该文件包含 { $count } 个牌组（{ $decks }）；其中所有卡片都会导入到上面指定的牌组。
# The package's media. $what lists them (import-media-*), $mb their size.
import-media = 该牌组包含 { $what }（{ $mb } MB）媒体
# Images in the package. $count is how many.
import-media-images =
    { $count ->
       *[other] { $count } 张图片
    }
# Sound clips in the package. $count is how many.
import-media-audio =
    { $count ->
       *[other] { $count } 段音频
    }
# Videos in the package. $count is how many.
import-media-video =
    { $count ->
       *[other] { $count } 个视频
    }
# Files of a kind Flash does not take (SVG, say). $count is how many.
import-media-unsupported =
    { $count ->
       *[other] { $count } 个不支持的文件
    }
# The review history was cut to the most recent ones. $max is how many.
import-history-capped = 复习历史记录仅保留最近的 { $max } 条；更早的记录留在 Anki 中
# The same in the phone app, which keeps fewer. $max is how many.
import-history-capped-phone = 手机端复习历史记录仅保留最近的 { $max } 条；更早的记录留在 Anki 中
# A CSV row with text that is not UTF-8, read by the phone app. $row is
# its line, $field which column.
import-row-invalid-utf8 = 行{ $row }：字段{ $field }中的 UTF-8 无效
# Notes left out. $count is how many.
import-notes-skipped =
    { $count ->
       *[other] 跳过 { $count } 条笔记（内容为空或格式不支持）
    }
# Cloze notes turned into cards. $notes notes became $cards cards (fewer
# than $notes when some blanks were empty).
import-cloze-expanded =
    { $notes ->
       *[other] { $notes } 条完形填空笔记生成
    } { $cards ->
       *[other] { $cards } 张卡片
    }
# Extra cards from the same notes (reversed cards and the like). $count
# is how many.
import-extra-layouts =
    { $count ->
       *[other] 包含 { $count } 种额外卡片格式（反向等）
    }
# Cards suspended in Anki stay out of study here too. $count is how
# many cards were suspended.
import-suspended =
    { $count ->
       *[other] { $count } 张卡片按 Anki 设置保持暂停
    }
# Extra cards that could not be read. $count is how many.
import-layouts-skipped =
    { $count ->
       *[other] 跳过 { $count } 张额外卡片（笔记格式无法读取）
    }
# A CSV row that could not be read. $row is its line, $error what the
# CSV reader said (English, technical).
import-row-unreadable = 行{ $row }: { $error }
# A CSV row with an empty side. $row is its line.
import-row-empty = 第 { $row } 行：正面或背面为空
# Something about one file inside the package. $file is its name, $what
# what is wrong with it.
import-about-file = { $file } : { $what }

## Why a file could not be imported

# The file has too many cards. $max is the limit.
import-too-many-cards = 该文件超过 { $max } 张卡片；每个文件最多导入 { $max } 张。请先在 Anki 中拆分文件
# The same, counted after reading. $count is the cards, $max the limit.
import-file-too-many-cards = 该文件有 { $count } 张卡片，超过每个文件 { $max } 张的导入上限；请先在 Anki 中拆分文件
# The file has no card to import. $count is the rows skipped.
import-nothing =
    { $count ->
       *[other] 未找到可导入的卡片（跳过 { $count } 行）
    }
# The CSV's first row could not be read.
import-csv-header-unreadable = 无法读取 CSV 的标题行
# The CSV lacks the needed columns; 'front' and 'back' are the column
# names it must have, as written (keep them).
import-csv-columns = CSV 需要“正面”和“背面”标题列
# The file is not an Anki package.
import-not-a-zip = 该文件不是 Anki 包（不是 zip 存档）
# The package's list of media is damaged.
import-media-list-damaged = 包的媒体列表已损坏且无法读取
# Part of the package could not be unpacked.
import-compressed-damaged = 压缩内容已损坏，无法解压
# The package holds too many files. $max is the limit.
import-too-many-files = 该包包含超过 { $max } 个文件，超出导入上限
# The package's list of media is too large.
import-media-list-large = 包的媒体列表太大
# A media file the package lists is not in it. $file is its name.
import-media-missing = 包中缺少 { $file }
# A media file could not be read. $file is its name.
import-media-unreadable = 无法从包中读取 { $file }
# A media file is over the size limit. $max is the limit in bytes.
import-media-too-large = 大于{ $max }字节限制
# A media file's content does not match its checksum.
import-media-hash = 内容与其声明的哈希值不匹配
# Unpacking would pass the size limit. $max is the limit in bytes.
import-decompress-limit = 解压缩超过{ $max }字节限制
# The Anki collection inside the package is damaged.
import-collection-damaged = 包中的 Anki 集合已损坏，无法读取
# The Anki collection inside the package is not laid out as expected.
import-collection-layout = 包中的 Anki 集合格式不符合预期
# The package could not be prepared for reading.
import-staging-failed = 该包无法暂存以供读取；再试一次
# The Anki collection is too large. $mb is the limit in megabytes.
import-collection-large = 文件内的集合大于{ $mb }MB
# The package holds no Anki collection.
import-no-collection = 文件内未找到 Anki 集合
# The preview is too old to import from.
import-expired = 预览已过期；再次上传
# The preview is another account's.
import-not-yours = 预览属于另一个用户
# Some cards name no deck and none was given.
import-needs-deck = 有些卡牌没有命名为牌组：命名一个牌组以将导入放入其中
# No deck name was given.
import-deck-required = 需要牌组名称
# An earlier import of the file stopped part way.
import-cut-off = 此次导入中途停止，部分卡片可能已在您的牌组中；再次导入前请先检查
# Too many uploads from one account at once.
import-busy-uploads = 一次上传太多；稍后再试
# Another import is running for the account.
import-busy = 另一个导入正在运行；稍后再试
# The deck name given is too long.
import-deck-too-long = 牌组名称太长
# The form came with no file.
import-no-file = 没有文件
# The file's name is too long.
import-name-too-long = 文件名太长
