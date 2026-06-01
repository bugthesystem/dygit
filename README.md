<p align="center">
  <img src="assets/logo.svg" width="84" height="84" alt="did-you-get-it">
</p>

<h1 align="center">did-you-get-it</h1>

<p align="center">
  A Claude Code plugin that quietly fixes messy prompts —<br>
  typos, dropped words, thumb-typed, half-awake — so Claude gets it the first time.
</p>

<p align="center">
  <a href="#install">Install</a> ·
  <a href="#how-it-works">How it works</a> ·
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

**It never gets in your way.** Any error — bad input, missing binary, cold start —
and the plugin stays silent; your prompt goes through untouched.

**It's private.** Everything runs locally. No network, no telemetry.

### Speed

The dictionary takes ~½ second to load — too slow to do per prompt. So a small
**daemon** loads it once and answers over a local socket in well under a
millisecond. The first prompt of a session uses the instant table while the daemon
warms up; it takes over from the next prompt on.

## Install

As a Claude Code plugin, from this repo:

```bash
claude plugin marketplace add bugthesystem/dygit
claude plugin install did-you-get-it@dygit-local
```

Or just the `dygi` binary, via Homebrew (Apple Silicon):

```bash
brew tap bugthesystem/dygit https://github.com/bugthesystem/dygit
brew install dygi
```

> Prebuilt binaries currently cover Apple Silicon; other platforms
> (darwin-x64, linux-x64/arm64) are added as releases gain those assets.

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

Binaries land in `bin/`; the hook picks the right one from `uname`.

> On macOS, copying a binary into `bin/` breaks its adhoc signature and the kernel
> kills it on launch — re-sign with `codesign --force --sign - <binary>`.
> `build-all.sh` does this for the darwin targets.

## License

MIT.

[symspell]: https://crates.io/crates/symspell
