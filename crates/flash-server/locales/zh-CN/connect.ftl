### The open core's words in English: the page that connects an AI
### assistant to Flash. Every message has a note above it: where it
### shows, who reads it, and what each variable holds.
###
### The assistants' own buttons and menus are messages of their own
### (ui-*), referenced from the steps: each must read exactly as that
### assistant's interface reads in the reader's language, copied from it,
### not translated.

## The page

# Browser tab title of the connect page, and its title for search engines.
connect-title = 连接您的 AI - Flash
# The connect page's description for search engines and link previews.
connect-description = 在大约一分钟内将 Flash 连接到 Claude、ChatGPT、Grok、Mistral、Gemini 或 Claude Code。
# Header button back to the app, for someone signed in.
connect-back = 返回 Flash
# Header button to the sign-in page.
connect-log-in = 登录
# Heading of the connect page.
connect-heading = 连接您的 AI
# Line under the heading.
connect-sub = 复制您的 Flash 地址，将其添加到您使用的助手中，然后登录 Flash 一次。大约一分钟。
# Title of the first step.
connect-step-copy = 1. 复制你的 Flash 地址
# Under it.
connect-step-copy-body = 下面的大多数助手都会要求您粘贴此内容。
# Button that copies the address.
connect-copy-url = 复制网址
# Title of the second step.
connect-step-add = 2. 在 AI 设置中添加 Flash
# Under it.
connect-step-add-body = 选择你正在使用的助手。
# A step for every assistant: Flash's sign-in page opens.
connect-sign-in = 当 Flash 询问时，登录 Flash。
# Footer: the server's address. $host is the address.
connect-footer = { $host }

## Claude

# Claude's menu that holds the connectors.
ui-claude-customize = 自定义
# Claude's list of connectors.
ui-claude-connectors = 连接器
# Claude's button that adds a connector of one's own.
ui-claude-add-custom = 添加自定义连接器
# Claude's button that saves it.
ui-claude-add = 添加
# Step 1 for Claude.
connect-claude-1-html = 打开<strong>{ ui-claude-customize }</strong>，然后打开<strong>{ ui-claude-connectors }</strong>。
# Step 2 for Claude: "+" is the plus button.
connect-claude-2-html = 按<strong>+ </strong>，然后点击<strong>{ ui-claude-add-custom } </strong>。
# Step 3 for Claude.
connect-claude-3-html = 粘贴地址，然后点击<strong>{ ui-claude-add }</strong>。
# The last step for Claude: turning Flash on in a chat.
connect-claude-5-html = 在任意聊天中，按<strong>+</strong>，打开<strong>{ ui-claude-connectors }</strong>，然后打开 Flash。
# Note under Claude's steps.
connect-claude-note = 适用于每个计划，包括免费。

## ChatGPT (as a custom MCP server)

# ChatGPT's settings.
ui-chatgpt-settings = 设置
# ChatGPT's section of plugins.
ui-chatgpt-plugins = 插件
# ChatGPT's ordinary chat mode, at the top of its home screen.
ui-chatgpt-chat = 聊天
# ChatGPT's work mode, beside it.
ui-chatgpt-work = 工作
# Warning before ChatGPT's steps: a paid plan is needed, and ChatGPT
# adds a server of one's own on the web only (OpenAI's help, 2026-10-14).
connect-chatgpt-warn-html = <strong> 开始之前：</strong> 这需要付费 ChatGPT 计划、Plus 或更高版本，并且 ChatGPT 在浏览器中，而不是在手机上。
# Step 1 for ChatGPT.
connect-chatgpt-1-html = 打开<strong>{ ui-chatgpt-settings }</strong>，然后打开<strong>{ ui-chatgpt-plugins }</strong>，然后点击<strong> + </strong>。
# Step 2 for ChatGPT: the "+" offers a custom MCP server; ChatGPT then
# shows a warning to accept before it creates the plugin. Its own labels
# for these are not quoted: they were not read from ChatGPT's
# translations.
connect-chatgpt-2 = 选择添加自定义 MCP 服务器，粘贴地址，接受警告并创建它。
# Step 3 for ChatGPT.
connect-chatgpt-3-html = 在插件中找到 Flash，然后点击<strong>+</strong>进行安装。
# Step 4 for ChatGPT.
connect-chatgpt-4-html = 在ChatGPT主屏幕上，从<strong>{ ui-chatgpt-chat }</strong>切换到顶部的<strong>{ ui-chatgpt-work }</strong>。
# Step 5 for ChatGPT: typing "@" offers the connected apps.
connect-chatgpt-5-html = 开始{ ui-chatgpt-work }聊天并输入<strong>@</strong>以选择 Flash。

## Grok

# Grok's button that adds a connector.
ui-grok-new-connector = 新连接器
# Grok's choice of a connector of one's own.
ui-grok-custom = 自定义
# Step 1 for Grok: its connectors page (an address; keep it).
connect-grok-1-html = 转至<strong>grok.com/connectors</strong>。
# Step 2 for Grok.
connect-grok-2-html = 按<strong>{ ui-grok-new-connector } </strong>，然后点击<strong>{ ui-grok-custom } </strong>。
# Step 3 for Grok.
connect-grok-3 = 粘贴地址并在 Flash 询问时登录。

## Mistral (Le Chat)

# Le Chat's list of connectors.
ui-mistral-connectors = 连接器
# Le Chat's button that adds a connector of one's own.
ui-mistral-add-custom = 添加自定义连接器
# Le Chat's button that saves it.
ui-mistral-add-connector = 添加连接器
# Step 1 for Mistral: its address (keep it).
connect-mistral-1-html = 转到<strong>chat.mistral.ai</strong>并打开<strong>{ ui-mistral-connectors }</strong>。
# Step 2 for Mistral: the connector is named Flash.
connect-mistral-2-html = 按<strong>{ ui-mistral-add-custom }</strong>，将其命名为<strong>Flash</strong>，粘贴地址作为服务器，然后点击<strong>{ ui-mistral-add-connector }</strong>。
# Step 4 for Mistral, with an example message that starts with @Flash
# (write the example as a person of the reader's language would type it).
connect-mistral-4-html = 在聊天中，以<strong>@Flash</strong>开始：“@Flash 在我的平台上开始学习课程。”
# Note under Mistral's steps. "Le Chat" is Mistral's assistant.
connect-mistral-note = 适用于米斯特拉尔的免费计划。 Le Chat 仅被要求“测验我”，有时会自己写一个测验，而不是使用你的卡片。

## Gemini

# Gemini's section of apps of one's own.
ui-gemini-custom-apps = 定制应用程序
# Gemini's button that goes to the next screen.
ui-gemini-next = 下一个
# Gemini's button that connects the app.
ui-gemini-connect = 连接
# Step 1 for Gemini: its apps page (an address; keep it).
connect-gemini-1-html = 在计算机上，转到<strong>gemini.google.com/apps</strong>。
# Step 2 for Gemini.
connect-gemini-2-html = 在<strong>{ ui-gemini-custom-apps }</strong>下，粘贴地址并按<strong>{ ui-gemini-next }</strong>两次，然后点击<strong>{ ui-gemini-connect }</strong>。
# Step 3 for Gemini.
connect-gemini-3-html = 当 Flash 出现询问时，登录 Flash，然后点击<strong>{ ui-gemini-connect }</strong>保存应用程序。
# Step 4 for Gemini.
connect-gemini-4-html = 在聊天中，从<strong>@Flash</strong>开始。
# Note under Gemini's steps; the date is when this was checked.
connect-gemini-note = 需要年满 18 岁，并在美国拥有个人 Google 帐户。截至 2026 年 10 月 11 日，Gemini 会将学习内容显示为文本，因此 Flash 卡片面板不会在那里出现。

## Coding tools and anything else

# Title of the card for Claude Code (a coding tool; keep its name).
connect-claude-code = 将 Flash 添加到 Claude 代码
# Under it: "/mcp" is a command, shown as code (keep it).
connect-claude-code-body-html = 将其粘贴到终端，然后运行 <span class="cn-mono">/mcp</span> 完成登录。
# Button that copies a terminal command.
connect-copy-command = 复制命令
# Title of the card for Codex (a coding tool; keep its name).
connect-codex = 将 Flash 添加到 Codex
# Under it.
connect-codex-body = 终端中的两个命令。第二个打开登录。
# Title of the last card: other apps.
connect-other = 用别的东西吗？
# Its text. MCP is the protocol's name (keep it).
connect-other-body = Flash 是一个普通的 MCP 服务器，因此相同的地址可以在 MCP 运行的任何地方工作：其他助手、编辑器、代理框架，无论您自己构建什么。将其粘贴到应用程序要求自定义 MCP 服务器的位置，并在它发送到此处时登录 Flash。
