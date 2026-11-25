#!/usr/bin/env python3
"""Print a Homebrew formula for a release, using the checksums in DIST."""
import os
import sys

version, dist = sys.argv[1], sys.argv[2]
repo = "https://github.com/Amet13/proofback/releases/download"


def sha(target):
    with open(os.path.join(dist, f"proofback-{target}.tar.gz.sha256")) as f:
        return f.read().split()[0]


print(f'''class Proofback < Formula
  desc "Map reviewer feedback on a PDF to the exact line of your LaTeX source"
  homepage "https://amet13.github.io/proofback/"
  version "{version}"
  license any_of: ["MIT", "Apache-2.0"]

  on_macos do
    on_arm do
      url "{repo}/v{version}/proofback-aarch64-apple-darwin.tar.gz"
      sha256 "{sha("aarch64-apple-darwin")}"
    end
    on_intel do
      url "{repo}/v{version}/proofback-x86_64-apple-darwin.tar.gz"
      sha256 "{sha("x86_64-apple-darwin")}"
    end
  end

  on_linux do
    on_arm do
      url "{repo}/v{version}/proofback-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "{sha("aarch64-unknown-linux-gnu")}"
    end
    on_intel do
      url "{repo}/v{version}/proofback-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "{sha("x86_64-unknown-linux-gnu")}"
    end
  end

  def install
    libexec.install Dir["*"]
    bin.write_exec_script libexec/"proofback"
  end

  test do
    assert_match version.to_s, shell_output("#{{bin}}/proofback --version")
  end
end''')
