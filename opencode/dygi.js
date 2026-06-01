// did-you-get-it (dygi) — opencode plugin.
//
// WHY THIS REWRITES INLINE (and only sometimes):
//   Claude Code and Cursor expose a side-channel ("additionalContext") that lets
//   us hand the model a *suggested* reading without touching what the user typed.
//   opencode has no such side-channel on the chat.message hook — the only lever
//   is the message parts themselves. So here we rewrite the user's text *in
//   place*. Because an inline rewrite is destructive (the user can't see the
//   original reading the model gets), we do it ONLY for high-confidence,
//   unambiguous typo fixes (verdict === "trivial"). For "interpret" (the engine
//   itself is unsure) or "clean" (nothing to fix) we leave the message exactly
//   as the user typed it — no surprising rewrites of ambiguous input.
//
//   The correction is done entirely by the shared `dygi` binary: a local,
//   offline, no-AI spell/segmentation pass (curated table + a resident symspell
//   daemon). This plugin is a thin pipe: it shells out to `dygi correct`, reads
//   one line of JSON, and conservatively applies the result. On ANY problem
//   (binary missing, timeout, non-JSON, multiple text parts, etc.) it does
//   nothing and never throws — a broken corrector must be invisible.

import { spawn } from "node:child_process";
import { existsSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

// Directory this plugin file lives in (opencode/), used to resolve the bundled
// binary fallback and the frequency dictionary relative to the repo.
const HERE = dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = dirname(HERE); // opencode/ -> repo root
const DICT_PATH = join(REPO_ROOT, "crate", "data", "freq_dict_en.txt");

// Hard cap on how long we wait for the binary. The corrector answers in
// microseconds once warm; anything slower means something is wrong — bail and
// leave the message untouched rather than ever stalling the chat.
const TIMEOUT_MS = 300;

// Maps Node's process.platform/arch to the prebuilt binary name shipped in
// ../bin (mirrors hooks/select-binary.sh, the Rust side's source of truth).
function bundledBinaryName() {
  const p = process.platform;
  const a = process.arch;
  if (p === "darwin" && a === "arm64") return "dygi-darwin-arm64";
  if (p === "darwin" && a === "x64") return "dygi-darwin-x64";
  if (p === "linux" && a === "x64") return "dygi-linux-x64";
  if (p === "linux" && a === "arm64") return "dygi-linux-arm64";
  return null; // unsupported platform: caller falls back to PATH
}

// Resolves which `dygi` to run. Order:
//   1. $DYGI_BIN if set (explicit override).
//   2. a prebuilt binary in ../bin for this platform, if present & shipped.
//   3. bare "dygi" — assume it is on PATH (brew install / cargo install).
// We never *verify* PATH here; if "dygi" is not installed, spawn fails and we
// no-op, which is the desired fail-safe behavior.
function resolveBinary() {
  if (process.env.DYGI_BIN) return process.env.DYGI_BIN;
  const name = bundledBinaryName();
  if (name) {
    const candidate = join(REPO_ROOT, "bin", name);
    if (existsSync(candidate)) return candidate;
  }
  return "dygi";
}

// Runs `dygi correct`, feeding `text` on stdin, and resolves to the parsed JSON
// ({ original, cleaned, verdict, changed }) or null on ANY failure: spawn error,
// non-zero exit, timeout, or unparseable output. Never rejects.
function correct(text) {
  return new Promise((resolve) => {
    let child;
    try {
      child = spawn(resolveBinary(), ["correct"], {
        // Point the spell-correction daemon at the bundled dictionary, exactly
        // as the Claude Code / Cursor hook wrappers do.
        env: { ...process.env, DYGI_DICT_PATH: DICT_PATH },
        stdio: ["pipe", "pipe", "ignore"],
      });
    } catch {
      resolve(null);
      return;
    }

    let out = "";
    let settled = false;
    const finish = (value) => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      try {
        child.kill();
      } catch {
        // already gone; ignore
      }
      resolve(value);
    };

    const timer = setTimeout(() => finish(null), TIMEOUT_MS);

    child.on("error", () => finish(null)); // e.g. binary not found on PATH
    child.stdout.on("data", (chunk) => {
      out += chunk.toString();
    });
    child.on("close", () => {
      const line = out.trim();
      if (!line) return finish(null);
      try {
        finish(JSON.parse(line));
      } catch {
        finish(null);
      }
    });

    try {
      child.stdin.write(text);
      child.stdin.end();
    } catch {
      finish(null);
    }
  });
}

// opencode plugin: a default-exported async factory that returns a hooks object.
// We register only the chat.message hook.
export default async function dygiPlugin() {
  return {
    // Fires when a new user message is received. `output.parts` is the live
    // array of message parts; mutating a part's `text` rewrites what the model
    // sees. We touch nothing else.
    "chat.message": async (_input, output) => {
      try {
        const parts = output?.parts;
        if (!Array.isArray(parts)) return;

        // Find the user's typed text. We only act when there is EXACTLY ONE text
        // part — the ordinary "typed a message" case. Zero text parts (e.g. a
        // bare attachment) or several (already-structured input) are left alone
        // so we never reassemble or mangle multi-part messages. Non-text parts
        // (files/attachments) are always preserved untouched.
        const textParts = parts.filter(
          (p) => p && p.type === "text" && typeof p.text === "string",
        );
        if (textParts.length !== 1) return;

        const part = textParts[0];
        const text = part.text;
        if (!text.trim()) return;

        const result = await correct(text);
        if (!result || typeof result.cleaned !== "string") return;

        // Conservative gate: rewrite inline ONLY for clear, high-confidence
        // fixes. Leave "interpret" (low confidence) and "clean" (no change)
        // exactly as the user typed them.
        if (result.verdict === "trivial" && result.cleaned !== text) {
          part.text = result.cleaned;
        }
      } catch {
        // Never let the corrector break message handling.
      }
    },
  };
}
