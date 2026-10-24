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
connect-title = Connect your AI - Flash
# The connect page's description for search engines and link previews.
connect-description = Attach Flash to Claude, ChatGPT, Grok, Mistral, Gemini, or Claude Code in about a minute.
# Header button back to the app, for someone signed in.
connect-back = Back to Flash
# Header button to the sign-in page.
connect-log-in = Log in
# Heading of the connect page.
connect-heading = Connect your AI
# Line under the heading.
connect-sub = Copy your Flash address, add it in the assistant you use, and sign in to Flash once. About a minute.
# Title of the first step.
connect-step-copy = 1. Copy your Flash address
# Under it.
connect-step-copy-body = Most assistants below ask you to paste this.
# Button that copies the address.
connect-copy-url = Copy URL
# Title of the second step.
connect-step-add = 2. Add Flash in your AI's settings
# Under it.
connect-step-add-body = Pick the one you use.
# A step for every assistant: Flash's sign-in page opens.
connect-sign-in = Sign in to Flash when it asks.
# Footer: the server's address. $host is the address.
connect-footer = { $host }

## Claude

# Claude's menu that holds the connectors.
ui-claude-customize = Customize
# Claude's list of connectors.
ui-claude-connectors = Connectors
# Claude's button that adds a connector of one's own.
ui-claude-add-custom = Add custom connector
# Claude's button that saves it.
ui-claude-add = Add
# Step 1 for Claude.
connect-claude-1-html = Open <strong>{ ui-claude-customize }</strong>, then <strong>{ ui-claude-connectors }</strong>.
# Step 2 for Claude: "+" is the plus button.
connect-claude-2-html = Press <strong>+</strong>, then <strong>{ ui-claude-add-custom }</strong>.
# Step 3 for Claude.
connect-claude-3-html = Paste the address, then press <strong>{ ui-claude-add }</strong>.
# The last step for Claude: turning Flash on in a chat.
connect-claude-5-html = In any chat, press <strong>+</strong>, open <strong>{ ui-claude-connectors }</strong>, and turn Flash on.
# Note under Claude's steps.
connect-claude-note = Works on every plan, free included.

## ChatGPT (as a custom MCP server)

# ChatGPT's settings.
ui-chatgpt-settings = Settings
# ChatGPT's section of plugins.
ui-chatgpt-plugins = Plugins
# ChatGPT's ordinary chat mode, at the top of its home screen.
ui-chatgpt-chat = Chat
# ChatGPT's work mode, beside it.
ui-chatgpt-work = Work
# Warning before ChatGPT's steps: a paid plan is needed, and ChatGPT
# adds a server of one's own on the web only (OpenAI's help, 2026-10-14).
connect-chatgpt-warn-html = <strong>Before you start:</strong> this needs a paid ChatGPT plan, Plus or higher, and ChatGPT in a browser, not on a phone.
# Step 1 for ChatGPT.
connect-chatgpt-1-html = Open <strong>{ ui-chatgpt-settings }</strong>, then <strong>{ ui-chatgpt-plugins }</strong>, and press <strong>+</strong>.
# Step 2 for ChatGPT: the "+" offers a custom MCP server; ChatGPT then
# shows a warning to accept before it creates the plugin. Its own labels
# for these are not quoted: they were not read from ChatGPT's
# translations.
connect-chatgpt-2 = Choose to add a custom MCP server, paste the address, accept the warning and create it.
# Step 3 for ChatGPT.
connect-chatgpt-3-html = Find Flash in your plugins and press <strong>+</strong> to install it.
# Step 4 for ChatGPT.
connect-chatgpt-4-html = On the ChatGPT home screen, switch from <strong>{ ui-chatgpt-chat }</strong> to <strong>{ ui-chatgpt-work }</strong> at the top.
# Step 5 for ChatGPT: typing "@" offers the connected apps.
connect-chatgpt-5-html = Start a { ui-chatgpt-work } chat and type <strong>@</strong> to pick Flash.

## Grok

# Grok's button that adds a connector.
ui-grok-new-connector = New Connector
# Grok's choice of a connector of one's own.
ui-grok-custom = Custom
# Step 1 for Grok: its connectors page (an address; keep it).
connect-grok-1-html = Go to <strong>grok.com/connectors</strong>.
# Step 2 for Grok.
connect-grok-2-html = Press <strong>{ ui-grok-new-connector }</strong>, then <strong>{ ui-grok-custom }</strong>.
# Step 3 for Grok.
connect-grok-3 = Paste the address and sign in to Flash when it asks.

## Mistral (Le Chat)

# Le Chat's list of connectors.
ui-mistral-connectors = Connectors
# Le Chat's button that adds a connector of one's own.
ui-mistral-add-custom = Add a custom connector
# Le Chat's button that saves it.
ui-mistral-add-connector = Add connector
# Step 1 for Mistral: its address (keep it).
connect-mistral-1-html = Go to <strong>chat.mistral.ai</strong> and open <strong>{ ui-mistral-connectors }</strong>.
# Step 2 for Mistral: the connector is named Flash.
connect-mistral-2-html = Press <strong>{ ui-mistral-add-custom }</strong>, name it <strong>Flash</strong>, paste the address as the server, and press <strong>{ ui-mistral-add-connector }</strong>.
# Step 4 for Mistral, with an example message that starts with @Flash
# (write the example as a person of the reader's language would type it).
connect-mistral-4-html = In a chat, start with <strong>@Flash</strong>: “@Flash start a study session on my deck.”
# Note under Mistral's steps. "Le Chat" is Mistral's assistant.
connect-mistral-note = Works on Mistral's free plan. Asked only to “quiz me”, Le Chat sometimes writes a quiz of its own instead of using your cards.

## Gemini

# Gemini's section of apps of one's own.
ui-gemini-custom-apps = Custom apps
# Gemini's button that goes to the next screen.
ui-gemini-next = Next
# Gemini's button that connects the app.
ui-gemini-connect = Connect
# Step 1 for Gemini: its apps page (an address; keep it).
connect-gemini-1-html = On a computer, go to <strong>gemini.google.com/apps</strong>.
# Step 2 for Gemini.
connect-gemini-2-html = Under <strong>{ ui-gemini-custom-apps }</strong>, paste the address and press <strong>{ ui-gemini-next }</strong> twice, then <strong>{ ui-gemini-connect }</strong>.
# Step 3 for Gemini.
connect-gemini-3-html = Sign in to Flash when it asks, then press <strong>{ ui-gemini-connect }</strong> to save the app.
# Step 4 for Gemini.
connect-gemini-4-html = In a chat, start with <strong>@Flash</strong>.
# Note under Gemini's steps; the date is when this was checked.
connect-gemini-note = Needs a personal Google account in the US, 18 or over. As of October 11, 2026, Gemini shows study sessions as text: Flash's card panel doesn't appear there.

## Coding tools and anything else

# Title of the card for Claude Code (a coding tool; keep its name).
connect-claude-code = Add Flash to Claude Code
# Under it: "/mcp" is a command, shown as code (keep it).
connect-claude-code-body-html = Paste this in your terminal, then run <span class="cn-mono">/mcp</span> to sign in.
# Button that copies a terminal command.
connect-copy-command = Copy command
# Title of the card for Codex (a coding tool; keep its name).
connect-codex = Add Flash to Codex
# Under it.
connect-codex-body = Two commands in your terminal. The second opens the sign-in.
# Title of the last card: other apps.
connect-other = Using something else?
# Its text. MCP is the protocol's name (keep it).
connect-other-body = Flash is a plain MCP server, so the same address works anywhere MCP does: other assistants, editors, agent frameworks, whatever you build yourself. Paste it wherever that app asks for a custom MCP server, and sign in to Flash when it sends you here.
