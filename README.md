<p align="center">
  <img src="assets/logo.svg" width="84" height="84" alt="did-you-get-it">
</p>

<h1 align="center">did-you-get-it</h1>

<p align="center">
  A Claude Code plugin that quietly fixes messy prompts —<br>
  typos, dropped words, thumb-typed, half-awake — so Claude gets it the first time.
</p>

---

## How it works

On every prompt, a small Rust binary cleans your text and tells Claude its best
reading. Claude opens its reply with one line — `✓ understood · <clean version>` —
then does the work. Your original prompt stays in the transcript, so the record
stays honest.

Correction runs in two layers:

- **Curated table** — unambiguous keyboard slips, fixed instantly and offline:
  `teh → the`, `wehn → when`, `usr → user`.
- **Spellchecker** — everything else goes through [symspell] against an 82k-word
  frequency dictionary. Real words are *in* that dictionary, so they're never
  "corrected" — `form`, `route`, and `stable` stay exactly as you typed them.

A misplaced-space repair also re-cuts adjacent tokens (`aut hbug → auth bug`),
but only when both halves are real words. When intent is genuinely ambiguous,
Claude interprets in context and may ask one short question instead of guessing.

**It never gets in your way.** Any error — bad input, missing binary, cold start —
means the plugin stays silent and your prompt goes through untouched.

## Speed

The spellchecker's dictionary takes ~½ second to load — too slow to do per
prompt. So a small **daemon** loads it once and answers over a local socket in
well under a millisecond. Until it's warm (the first prompt of a session), the
curated table covers you; the daemon takes over from the next prompt on.

## Commands

- `/did-you-get-it:history [N]` — recent cleanups, `original → cleaned`
- `/did-you-get-it:stats` — totals, top tokens, interpretation rate
- `/did-you-get-it:toggle [on·off·verbose·quiet·aggressive·gentle]` — settings (no arg shows state)
- `/did-you-get-it:undo` — your last original prompt, verbatim, to re-send

## Data

Everything is local, under `~/.claude/plugins/data/did-you-get-it/` —
`events.jsonl` for history, `config.json` for settings. No network, no telemetry.

## Install

Homebrew (Apple Silicon) — installs the `dygi` binary and dictionary from a
release:

```bash
brew tap bugthesystem/dygit https://github.com/bugthesystem/dygit
brew install dygi
```

> Requires the repository to be public so Homebrew can fetch the release asset.
> Other platforms (darwin-x64, linux-x64/arm64) are added as releases gain those
> binaries.

## Build

```bash
./scripts/build-all.sh          # all platforms (needs cross toolchains)
```

Binaries land in `bin/` (`dygi-darwin-arm64`, `dygi-darwin-x64`, `dygi-linux-x64`,
`dygi-linux-arm64`); the hook picks the right one from `uname`.

> On macOS, copying a binary into `bin/` breaks its adhoc signature and the
> kernel kills it on launch — re-sign with `codesign --force --sign - <binary>`.
> `build-all.sh` already does this for the darwin targets.

[symspell]: https://crates.io/crates/symspell
