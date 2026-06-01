# did-you-get-it for opencode

A thin [opencode](https://github.com/sst/opencode) plugin that fixes obvious
typos in your prompts *before* they reach the model — locally, with no AI.

It shells out to the shared `dygi` binary (the same engine used by the Claude
Code and Cursor integrations) and applies its correction.

## What it does (and what it deliberately does not)

opencode plugins have no "additional context" side-channel — the only way to
influence the model from the `chat.message` hook is to change the message text
itself. So this plugin **rewrites your message inline**, but only when it is
safe to do so:

- **`trivial`** (clear, high-confidence typo/spacing fix, e.g. `fix teh bug` →
  `fix the bug`): rewritten inline.
- **`interpret`** (the engine itself is unsure how to read it): **left exactly
  as you typed it.** No surprising rewrites of ambiguous input.
- **`clean`** (nothing to fix): left as-is.

This is the key semantic difference from the Claude Code / Cursor integrations.
There, the original text is preserved and a *suggested* reading is passed to the
model as side context. Here there is no side channel, so we rewrite in place —
and therefore only for unambiguous fixes.

It also only acts on a message that has exactly one text part (the ordinary
"typed a message" case). Attachments and other non-text parts are always
preserved untouched. On any error (binary missing, timeout, bad output) it does
nothing and never throws.

## Install

1. **Install the `dygi` binary** so it is on your `PATH`:

   ```sh
   brew tap bugthesystem/dygit https://github.com/bugthesystem/dygit
   brew install dygi
   ```

   Or set `DYGI_BIN` to an explicit path, or run the plugin from a checkout of
   the repo (it finds the prebuilt binary in `../bin/` automatically).

2. **Install the plugin.** Copy or symlink `dygi.js` into your opencode plugins
   directory:

   ```sh
   # global (all projects)
   mkdir -p ~/.config/opencode/plugins
   ln -s "$(pwd)/dygi.js" ~/.config/opencode/plugins/dygi.js

   # or per-project
   mkdir -p .opencode/plugins
   ln -s "$(pwd)/dygi.js" .opencode/plugins/dygi.js
   ```

3. Restart opencode. Typos in `trivial`-confidence prompts are now fixed inline.

## Binary resolution order

The plugin looks for `dygi` in this order:

1. `$DYGI_BIN` (explicit override).
2. A prebuilt binary in `../bin/dygi-<platform>` (when running from the repo).
3. `dygi` on your `PATH`.

If none resolve, the plugin simply does nothing.

## Notes

- The correction is 100% local and offline (a curated typo table plus a resident
  symspell daemon over the bundled 82k-word dictionary). No network, no AI.
- The dictionary is found automatically: the plugin sets `DYGI_DICT_PATH` to
  `../crate/data/freq_dict_en.txt` relative to itself.
