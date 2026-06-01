# Homebrew formula for did-you-get-it (the `dygi` binary + dictionary).
#
# Installs the prebuilt binary and the frequency dictionary from a GitHub
# release — no Rust toolchain required. The Claude Code plugin files (hooks,
# commands, manifest) are also placed in the formula's prefix for reference;
# wiring the plugin into Claude Code is a separate step (see `caveats`).
#
# Tap + install:
#   brew tap bugthesystem/dygit https://github.com/bugthesystem/dygit
#   brew install dygi
#
# Today only Apple Silicon (arm64) ships a prebuilt binary; other platforms are
# added as releases gain those assets. The structure below makes that a matter
# of filling in url/sha256 per `on_<os>`/`on_<arch>` block.
class Dygi < Formula
  desc "Cleans messy Claude Code prompts (typos, dropped words) before Claude sees them"
  homepage "https://github.com/bugthesystem/dygit"
  version "0.1.0"
  license "MIT"

  on_macos do
    on_arm do
      url "https://github.com/bugthesystem/dygit/releases/download/v0.1.0/dygi-0.1.0-darwin-arm64.tar.gz"
      sha256 "56cb0d31f67625be5f7288456a9b9b757f8280e7e7d3943a1f01df2c996d3a37"
    end
    on_intel do
      url "https://github.com/bugthesystem/dygit/releases/download/v0.1.0/dygi-0.1.0-darwin-x64.tar.gz"
      sha256 "8f2c96ef6175cb8b7936930015d192fa7448a8c5898c71a5a10239e8abd36562"
    end
  end

  on_linux do
    on_intel do
      url "https://github.com/bugthesystem/dygit/releases/download/v0.1.0/dygi-0.1.0-linux-x64.tar.gz"
      sha256 "89bc95aaeedb81f1d6c1f4edc19aadd2d9912c75ddaa690cd1abff21bdc755fe"
    end
    on_arm do
      url "https://github.com/bugthesystem/dygit/releases/download/v0.1.0/dygi-0.1.0-linux-arm64.tar.gz"
      sha256 "6b4113bdcf9ef09d55457945cc0b9162ab50483051d185babc24997d745cde33"
    end
  end

  def install
    # The tarball unpacks into a single top-level directory; Homebrew strips it,
    # so the binary and data land at the staging root.
    bin.install "dygi"
    pkgshare.install "freq_dict_en.txt"

    # Keep the plugin assets alongside the formula prefix for users who want to
    # wire the Claude Code plugin manually (see caveats).
    pkgshare.install "hooks" if File.directory?("hooks")
    pkgshare.install "commands" if File.directory?("commands")
    pkgshare.install ".claude-plugin" if File.directory?(".claude-plugin")
  end

  def caveats
    <<~EOS
      The dictionary was installed to:
        #{opt_pkgshare}/freq_dict_en.txt

      `dygi` finds it automatically when DYGI_DICT_PATH is set. To use it
      standalone, export:
        export DYGI_DICT_PATH="#{opt_pkgshare}/freq_dict_en.txt"

      To use it as a Claude Code plugin, install the plugin from the repo
      (the hook wrapper sets DYGI_DICT_PATH for you):
        https://github.com/bugthesystem/dygit
    EOS
  end

  test do
    # The hook reads a UserPromptSubmit JSON payload on stdin and prints a
    # correction note. With a typo present it must mention the fixed reading.
    ENV["DYGI_DICT_PATH"] = "#{pkgshare}/freq_dict_en.txt"
    out = pipe_output(
      "#{bin}/dygi hook",
      %({"prompt":"teh test","cwd":"/tmp","session_id":"t"}),
    )
    assert_match "the test", out
  end
end
