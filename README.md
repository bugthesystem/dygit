<p align="center">
  <img src="assets/logo.svg" width="88" height="88" alt="did-you-get-it">
</p>

<h1 align="center">did-you-get-it</h1>

<p align="center">
  <em>Quietly fixes messy prompts — typos, dropped words, thumb-typed, half-awake —<br>
  so Claude gets it the first time.</em>
</p>

<p align="center">
  <a href="https://github.com/bugthesystem/dygit/releases"><img src="https://img.shields.io/github/v/release/bugthesystem/dygit?color=22c55e&label=release" alt="release"></a>
  <img src="https://img.shields.io/badge/license-MIT-blue" alt="MIT">
  <img src="https://img.shields.io/badge/rust-idiomatic-orange" alt="Rust">
  <img src="https://img.shields.io/badge/spell--check-0%20tokens-22c55e" alt="0 tokens">
</p>

<p align="center">
  <a href="#install">Install</a> ·
  <a href="#how-it-works">How it works</a> ·
  <a href="#why">Why</a> ·
  <a href="#commands">Commands</a> ·
  <a href="#build">Build</a>
</p>

---

You type:

```
fix teh aut hbug wehn usr lgs ut
```

Claude reads:

```
✓ understood · fix the auth bug when user logs out
```

…and gets to work. Your original text stays in the transcript — nothing is hidden,
nothing is sent anywhere.

## How it works

On every prompt, a small Rust binary cleans your text and hands Claude its best
reading. Claude opens its reply with one line — `✓ understood · <clean version>` —
then does the work. Correction runs in two layers:

- **Curated table** — unambiguous keyboard slips, fixed instantly and offline:
  `teh → the`, `wehn → when`, `usr → user`.
- **Spellchecker** — everything else goes through [symspell] against an 82k-word
  frequency dictionary. Real words are *in* that dictionary, so they're never
  "corrected" — `form`, `route`, and `stable` stay exactly as you typed them.

A misplaced-space repair also re-cuts adjacent tokens (`aut hbug → auth bug`), but
only when both halves are real words. When intent is genuinely ambiguous, Claude
interprets in context and may ask one short question instead of guessing.

> **Never in the way.** Any error — bad input, missing binary, cold start — and the
> plugin stays silent; your prompt goes through untouched.

### Speed

The dictionary takes ~½ second to load — too slow to do per prompt. So a small
**daemon** loads it once and answers over a local socket in well under a
millisecond. The first prompt of a session uses the instant table while the daemon
warms up; it takes over from the next prompt on.

## Why

| | |
|---|---|
| 🧠 **No AI for spell-check** | Pure classical code — a lookup table + [symspell]. The model is never asked to fix a typo. |
| 💸 **No token cost** | Spell-checking is free local computation. A messy prompt adds a one-line hint (~tens of tokens) to the turn you were already sending; a clean prompt adds nothing. |
| 🔒 **Private** | Everything runs on your machine. No network, no telemetry, no API key. |
| ⚡ **Fast** | Corrections resolve in well under a millisecond. |

The idea: use cheap deterministic code for the mechanical work, and spend the
model's attention only on the genuinely ambiguous calls — without an extra request.

## Install

**As a Claude Code plugin** (recommended):

```bash
claude plugin marketplace add bugthesystem/dygit
claude plugin install did-you-get-it@dygit-local
```

**Or just the `dygi` binary**, via Homebrew (macOS & Linux, arm64 + x64):

```bash
brew tap bugthesystem/dygit https://github.com/bugthesystem/dygit
brew install dygi
```

### Cursor

The same binary works in [Cursor](https://cursor.com) (v1.7+) — its
`beforeSubmitPrompt` hook speaks the same protocol. Clone the repo and point a
hook at it:

```jsonc
// .cursor/hooks.json  (project)  or  ~/.cursor/hooks.json  (global)
{
  "version": 1,
  "hooks": {
    "beforeSubmitPrompt": [
      { "command": "bash /absolute/path/to/dygit/hooks/run.sh" }
    ]
  }
}
```

A ready-to-copy `.cursor/hooks.json` ships in this repo (uses a project-relative
path). The cleaned reading is injected as `additionalContext`, exactly as in
Claude Code — same engine, no AI, no token cost for the spell-check.

## Commands

| Command | Does |
|---|---|
| `/did-you-get-it:history [N]` | recent cleanups, `original → cleaned` |
| `/did-you-get-it:stats` | totals, top tokens, interpretation rate |
| `/did-you-get-it:toggle [on·off·verbose·quiet·aggressive·gentle]` | settings (no arg shows state) |
| `/did-you-get-it:undo` | your last original prompt, verbatim, to re-send |

Data lives under `~/.claude/plugins/data/did-you-get-it/` — `events.jsonl` for
history, `config.json` for settings.

## Build

```bash
./scripts/build-all.sh          # all platforms (needs cross toolchains)
```

Binaries land in `bin/`; the hook picks the right one from `uname`. Releases are
built for all four platforms by CI on every tag.

> On macOS, copying a binary into `bin/` breaks its adhoc signature and the kernel
> kills it on launch — re-sign with `codesign --force --sign - <binary>`.
> `build-all.sh` does this for the darwin targets.

## License

[MIT](LICENSE) © bugthesystem

[symspell]: https://crates.io/crates/symspell
