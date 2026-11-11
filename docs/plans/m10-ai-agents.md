# M10 AI through agents and subscriptions

Goal: an author who already pays for Claude or ChatGPT connects CourseCraft to Claude Code or Codex with a click, and the built-in AI features (document to course, alt text, translation, connection test) can run through that plan instead of an API key. Nobody needs a terminal.

One window, **AI Settings**, with two tabs:

- **Agents**: tools that drive CourseCraft from outside, through its MCP server.
- **AI features**: what powers CourseCraft's own AI features.

Help ▸ Connect an AI agent… opens the same window on the Agents tab. The old window's copy lines move into a "Set up by hand" section at the bottom of the Agents tab.

## Step 1: Agents tab

Agents, in this order: Claude Code, Codex, Claude Desktop, Cursor, Gemini CLI.

Each row shows the name, a status, and buttons:

| Status | Meaning | Buttons |
|---|---|---|
| Not found | The CLI or app isn't on this computer | a "How to install" link (opens the vendor's page) |
| Found | Installed, CourseCraft not added | **Connect** |
| Connected | CourseCraft is in its MCP list | **Test**, **Disconnect** |
| Problem | A command failed | the plain sentence, **Try again** |

How each is done (desktop only; the web editor shows the "Set up by hand" lines only):

- **Claude Code**: `claude mcp add --transport stdio --scope user coursecraft -- <exe> mcp --attach`; status from `claude mcp get coursecraft` (exit code); remove with `claude mcp remove --scope user coursecraft`.
- **Codex**: `codex mcp add coursecraft -- <exe> mcp --attach`; `codex mcp get coursecraft`; `codex mcp remove coursecraft`.
- **Gemini CLI**: `gemini mcp add --scope user coursecraft <exe> mcp --attach`; status from `gemini mcp list` output containing `coursecraft`; `gemini mcp remove --scope user coursecraft`.
- **Claude Desktop**: JSON file `claude_desktop_config.json` (macOS `~/Library/Application Support/Claude/`, Windows `%APPDATA%\Claude\`). **Cursor**: `~/.cursor/mcp.json`. Both use `{"mcpServers": {"coursecraft": {"command": <exe>, "args": ["mcp", "--attach"]}}}`. Found = the app's folder exists. Connect merges the entry and keeps every other key exactly; a file that doesn't parse is never overwritten (say so). Before writing, copy the old file to `<name>.coursecraft-backup`. Write atomically. Disconnect removes only `coursecraft`. Say "Restart Claude Desktop to pick it up."
- **Test** runs the agent's status command again and reports "Connected. Ask <name> to use CourseCraft." Claude Desktop and Cursor: test reads the file back.

Finding CLIs: macOS apps don't get the shell's `PATH`. Look in `PATH`, then `~/.local/bin`, `~/.claude/local`, `/opt/homebrew/bin`, `/usr/local/bin`, `~/.npm-global/bin`, `~/.bun/bin`, `~/.volta/bin`, every `~/.nvm/versions/node/*/bin`, and on Windows `%APPDATA%\npm` (`.cmd`) and `%USERPROFILE%\.local\bin` (`.exe`). Run found CLIs with that directory and the other found directories prepended to `PATH`, so node-based CLIs find `node`. Stdin closed, a 30 s timeout, output capped.

Automation off: the tab says agents can't reach the app until it's on, with a Turn on button, as now.

Where the code goes: pure parts (command lines, the JSON merge and removal, reading status output, the search directory list) in `coursecraft-ai::agents`, unit tested. Running processes and touching files in `apps/coursecraft`, behind a trait so tests use a fake. The UI only sends `EditorRequest`s and shows the status the app hands back. Detection runs when the tab opens and after each action, on a background thread, never blocking a frame.

## Step 2: AI features through a subscription

The AI features tab gets **Use:** with three choices:

- **Claude Code (your Claude plan)**: needs Claude Code found.
- **Codex (your ChatGPT plan)**: needs Codex found.
- **An API endpoint**: today's fields, unchanged.

The choice is saved in the desktop prefs (`AiPrefs`), default "An API endpoint" so existing prefs load as before. The web editor offers only the endpoint.

Running a chat request through an agent (desktop):

- The request's messages become one prompt: the system text, then the user text, then "Reply with only the answer, no commentary." Images (alt text) are written to a fresh temporary folder and named in the prompt.
- Claude Code: `claude -p --output-format json --tools Read` with the prompt on stdin, working directory the temp folder; the answer is the JSON's `result`; `is_error: true` is a failure.
- Codex: `codex exec --skip-git-repo-check --sandbox read-only --output-last-message <tmp>/answer.txt -` with the prompt on stdin, `--image <file>` per image, working directory the temp folder; the answer is the file's text.
- No course file, project folder or key is ever given to the agent. The temp folder is removed afterwards.
- Timeout 10 minutes; Cancel kills the process. A non-zero exit becomes a plain sentence: "Claude Code isn't signed in. Run `claude` once in a terminal to sign in." when the output mentions login or auth, otherwise "Claude Code stopped with an error." plus at most 200 characters of its output.
- The answer comes back as a normal `ChatReply`, so outline, alt text and translation code doesn't change. Test connection asks for "Reply with OK".
- Captions and narration need audio models: with an agent chosen they say "Captions need an API endpoint or ElevenLabs" and open the tab.
- The "what's sent and where" notice names the agent: "…to Claude Code on this computer, which sends it to Anthropic under your plan."

## Done bar

- Tests: argv for every agent and action; JSON merge keeps other servers and refuses a broken file; the backup is written; status parsing; agent prompts and answer parsing for both agents; timeout and cancel; error sentences; prefs round trip with the old format.
- By hand (Claude, on this Mac where Claude Code and Codex are installed): Connect, Test and Disconnect for Claude Code and Codex; document to course and alt text through each agent.
