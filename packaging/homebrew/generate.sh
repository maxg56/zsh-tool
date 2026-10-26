#!/usr/bin/env bash
# Prints the Homebrew formula for a release, using the checksums of the
# archives built by the release workflow.
#
# Usage: packaging/homebrew/generate.sh <tag> <directory with *.tar.gz.sha256>
set -euo pipefail

if [[ $# -ne 2 ]]; then
    echo "usage: $0 <tag> <dist directory>" >&2
    exit 2
fi
tag=$1
dist=$2
version=${tag#v}
base="https://github.com/maxg56/zsh-tool/releases/download/$tag"

# Checksums are read up front: a failure inside the heredoc's command
# substitutions would not stop the script.
declare -A sha
for target in aarch64-apple-darwin x86_64-apple-darwin \
    aarch64-unknown-linux-musl x86_64-unknown-linux-musl; do
    file="$dist/zsh-tool-$tag-$target.tar.gz.sha256"
    if [[ ! -f $file ]]; then
        echo "missing $file" >&2
        exit 1
    fi
    sha[$target]=$(cut -d' ' -f1 "$file")
done

cat <<FORMULA
class ZshTool < Formula
  desc "Fast zsh configuration manager compatible with Oh My Zsh plugins"
  homepage "https://github.com/maxg56/zsh-tool"
  version "$version"
  license "MIT"

  on_macos do
    on_arm do
      url "$base/zsh-tool-$tag-aarch64-apple-darwin.tar.gz"
      sha256 "${sha[aarch64-apple-darwin]}"
    end
    on_intel do
      url "$base/zsh-tool-$tag-x86_64-apple-darwin.tar.gz"
      sha256 "${sha[x86_64-apple-darwin]}"
    end
  end

  on_linux do
    on_arm do
      url "$base/zsh-tool-$tag-aarch64-unknown-linux-musl.tar.gz"
      sha256 "${sha[aarch64-unknown-linux-musl]}"
    end
    on_intel do
      url "$base/zsh-tool-$tag-x86_64-unknown-linux-musl.tar.gz"
      sha256 "${sha[x86_64-unknown-linux-musl]}"
    end
  end

  depends_on "git"
  depends_on "zsh"

  def install
    bin.install "zsh-tool"
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/zsh-tool --version")
    system "#{bin}/zsh-tool", "init", "zsh"
  end
end
FORMULA
