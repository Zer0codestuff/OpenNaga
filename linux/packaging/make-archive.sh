#!/bin/sh
# Packs the release binary into opennaga-<version>-<arch>-linux.tar.gz,
# with the data files, install.sh, the README and the license.
# Usage: packaging/make-archive.sh [output-dir]   (after cargo build --release)

set -eu
here=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
out="${1:-$here/target/dist}"
version=$(sed -n 's/^version = "\(.*\)"/\1/p' "$here/Cargo.toml" | head -n 1)
arch=$(uname -m)
name="opennaga-$version-$arch-linux"
stage=$(mktemp -d)
trap 'rm -rf "$stage"' EXIT

mkdir -p "$stage/$name" "$out"
install -m755 "$here/target/release/opennaga" "$stage/$name/opennaga"
install -m755 "$here/install.sh" "$stage/$name/install.sh"
cp -r "$here/data" "$stage/$name/data"
install -m644 "$here/README.md" "$stage/$name/README.md"
install -m644 "$here/../LICENSE" "$stage/$name/LICENSE"
tar -C "$stage" --owner=0 --group=0 --sort=name -czf "$out/$name.tar.gz" "$name"
(cd "$out" && sha256sum "$name.tar.gz" > "$name.tar.gz.sha256")
echo "$out/$name.tar.gz"
