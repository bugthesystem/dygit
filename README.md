<p align="center">
  <img src="assets/logo.svg" width="80" height="80" alt="did-you-get-it">
</p>

<h1 align="center">did-you-get-it</h1>

<p align="center">
  Fixes typos in your prompts before your AI editor sees them. Locally, no AI, no token cost.
</p>

<p align="center">
  Works in Claude Code, Cursor, and OpenCode.
</p>

<p align="center">
  <a href="https://github.com/bugthesystem/dygit/releases"><img src="https://img.shields.io/github/v/release/bugthesystem/dygit?label=release" alt="release"></a>
  <img src="https://img.shields.io/badge/license-MIT-blue" alt="MIT">
</p>

---

You type:

```
fix teh aut hbug wehn usr lgs ut
```

The model reads:

```
fix the auth bug when user logs out
```

Your original text stays in the transcript. Nothing is sent anywhere.

## How it works

A small Rust binary runs on each prompt and corrects it in two passes:

1. **A curated table** of unambiguous keyboard slips — `teh → the`, `wehn → when`,
   `usr → user` — fixed instantly, offline.
2. **[symspell]** against an 82k-word frequency dictionary for everything else.
   Real words are in the dictionary, so they're left alone: `form`, `route`, and
   `stable` are never "corrected" into something else.

A space-repair pass also re-joins split tokens (`aut hbug → auth bug`), but only
when both halves are real words. Ambiguous input is left untouched — the binary
won't guess.

In Claude Code and Cursor the cleaned reading is passed to the model as
side-channel context, so your original prompt stays visible. In OpenCode (which has
no such channel) the message is rewritten inline, and only for high-confidence
fixes.

The 82k-word dictionary takes ~½s to load, so a resident daemon loads it once and
answers over a Unix socket in under a millisecond. Until it's warm, the table
covers you. On any error the binary stays silent and your prompt goes through
untouched.

## Why

- **No AI in the spell-check.** It's a lookup table and an edit-distance algorithm.
  The model is never asked to fix a typo.
- **No token cost.** Correction is local computation. A messy prompt adds a short
  hint to the request you were already sending; a clean prompt adds nothing.
- **Private.** Runs on your machine. No network, no telemetry, no API key.
- **One binary, three editors.** Same engine behind every integration.

## Install

Get the binary (macOS & Linux, arm64 + x64):

```sh
brew tap bugthesystem/dygit https://github.com/bugthesystem/dygit
brew install dygi
```

Or [build from source](#build). Then wire it into your editor.

### Claude Code

```sh
claude plugin marketplace add bugthesystem/dygit
claude plugin install did-you-get-it@dygit-local
```

Installs the prompt hook and four slash commands ([below](#commands)).

### Cursor

Cursor 1.7+ has an equivalent prompt hook. Point `.cursor/hooks.json` (project) or
`~/.cursor/hooks.json` (global) at the wrapper:

```jsonc
{
  "version": 1,
  "hooks": {
    "beforeSubmitPrompt": [
      { "command": "bash /absolute/path/to/dygit/hooks/run.sh" }
    ]
  }
}
```

A ready-to-copy `.cursor/hooks.json` ships in this repo.

### OpenCode

```sh
cp opencode/dygi.js ~/.config/opencode/plugins/    # or .opencode/plugins/ per project
```

OpenCode rewrites the message inline (no side-channel), so it only acts on
high-confidence fixes. See [`opencode/README.md`](opencode/README.md).

## Commands

Claude Code only:

| Command | Does |
|---|---|
| `/did-you-get-it:history [N]` | recent cleanups, `original → cleaned` |
| `/did-you-get-it:stats` | totals, top tokens, interpretation rate |
| `/did-you-get-it:toggle [on·off·verbose·quiet·aggressive·gentle]` | settings (no arg shows state) |
| `/did-you-get-it:undo` | last original prompt, verbatim, to re-send |

State lives in `~/.claude/plugins/data/did-you-get-it/`.

The binary also works standalone:

```sh
echo "fix teh bug" | dygi correct
# {"original":"fix teh bug","cleaned":"fix the bug","verdict":"trivial","changed":true}
```

## Build

```sh
./scripts/build-all.sh          # all platforms (needs cross toolchains)
```

Binaries land in `bin/`; the hook picks the right one from `uname`. CI builds all
four platforms on every tag.

> macOS strips a binary's adhoc signature when it's copied, and the kernel then
> kills it on launch. Re-sign with `codesign --force --sign - <binary>`;
> `build-all.sh` does this for the darwin targets.

## License

[MIT](LICENSE) © bugthesystem

[symspell]: https://crates.io/crates/symspell
