#!/bin/sh
# Install the latest proofback release into ~/.local (no root needed).
# Usage: curl -fsSL https://github.com/Amet13/proofback/releases/latest/download/install.sh | sh
set -eu

repo="Amet13/proofback"
prefix="${PROOFBACK_PREFIX:-$HOME/.local}"
version="${PROOFBACK_VERSION:-latest}"

case "$(uname -s)" in
  Linux) os="unknown-linux-gnu" ;;
  Darwin) os="apple-darwin" ;;
  *) echo "proofback: unsupported OS $(uname -s); download a release manually" >&2; exit 1 ;;
esac
case "$(uname -m)" in
  x86_64 | amd64) arch="x86_64" ;;
  arm64 | aarch64) arch="aarch64" ;;
  *) echo "proofback: unsupported architecture $(uname -m)" >&2; exit 1 ;;
esac
name="proofback-$arch-$os"
if [ "$version" = "latest" ]; then
  base="https://github.com/$repo/releases/latest/download"
else
  base="https://github.com/$repo/releases/download/$version"
fi

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
curl -fsSL -o "$tmp/$name.tar.gz" "$base/$name.tar.gz"
curl -fsSL -o "$tmp/$name.tar.gz.sha256" "$base/$name.tar.gz.sha256"
cd "$tmp"
if command -v sha256sum > /dev/null; then
  sha256sum -c "$name.tar.gz.sha256"
else
  shasum -a 256 -c "$name.tar.gz.sha256"
fi
tar xzf "$name.tar.gz"
mkdir -p "$prefix/lib/proofback" "$prefix/bin"
rm -rf "$prefix/lib/proofback"
mv "$name" "$prefix/lib/proofback"
ln -sf "$prefix/lib/proofback/proofback" "$prefix/bin/proofback"
echo "proofback installed to $prefix/bin/proofback"
case ":$PATH:" in *":$prefix/bin:"*) ;; *) echo "Add $prefix/bin to your PATH." ;; esac
