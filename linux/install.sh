#!/bin/sh
# Builds and installs OpenNaga for Linux.
#
#   ./install.sh --user            build, then install to ~/.local (no root)
#   ./install.sh                   build as your user first, then:
#   sudo ./install.sh --no-build   install to /usr/local
#   ./install.sh --uninstall [--user]
#
# From a release archive, which holds a ready-built binary, the same
# commands install it without building.
#
# Options:
#   --user          install to ~/.local instead of /usr/local
#   --prefix DIR    install below DIR (default /usr/local)
#   --no-build      use the existing target/release/opennaga
#   --udev          also install the device access rule (needs root)
#   --uninstall     remove what this script installed (profiles stay)
#
# DESTDIR and UDEVDIR are honored for packaging. Without --udev, the
# Status pane in the app installs the rule with an administrator prompt.

set -eu

APP_ID=io.github.zer0codestuff.OpenNaga
here=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
prefix=/usr/local
build=1
udev=0
uninstall=0
user=0

while [ $# -gt 0 ]; do
    case "$1" in
        --user) user=1; prefix="$HOME/.local" ;;
        --prefix) shift; prefix="$1" ;;
        --prefix=*) prefix="${1#*=}" ;;
        --no-build) build=0 ;;
        --udev) udev=1 ;;
        --uninstall) uninstall=1 ;;
        -h|--help) sed -n '2,21p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
        *) echo "Unknown option: $1" >&2; exit 2 ;;
    esac
    shift
done

destdir="${DESTDIR:-}"
bindir="$destdir$prefix/bin"
datadir="$destdir$prefix/share"
udevdir="$destdir${UDEVDIR:-/etc/udev/rules.d}"
binary="$here/target/release/opennaga"
if [ ! -f "$here/Cargo.toml" ]; then
    # Release archive: the binary sits next to this script.
    binary="$here/opennaga"
    build=0
fi

icon_sizes="16 22 24 32 48 64 128 256 512"

refresh_caches() {
    [ -n "$destdir" ] && return 0
    if command -v gtk-update-icon-cache >/dev/null 2>&1 && [ -f "$datadir/icons/hicolor/index.theme" ]; then
        gtk-update-icon-cache -q -t -f "$datadir/icons/hicolor" 2>/dev/null || true
    fi
    if command -v update-desktop-database >/dev/null 2>&1; then
        update-desktop-database -q "$datadir/applications" 2>/dev/null || true
    fi
}

reload_udev() {
    [ -n "$destdir" ] && return 0
    if command -v udevadm >/dev/null 2>&1; then
        udevadm control --reload-rules
        udevadm trigger --action=change --subsystem-match=input --subsystem-match=hidraw --subsystem-match=misc
    fi
}

if [ "$uninstall" = 1 ]; then
    rm -f "$bindir/opennaga" \
        "$datadir/applications/$APP_ID.desktop" \
        "$datadir/metainfo/$APP_ID.metainfo.xml" \
        "$datadir/icons/hicolor/scalable/apps/$APP_ID.svg"
    for size in $icon_sizes; do
        rm -f "$datadir/icons/hicolor/${size}x${size}/apps/$APP_ID.png"
    done
    if [ "$udev" = 1 ]; then
        rm -f "$udevdir/60-opennaga.rules"
        reload_udev
    fi
    refresh_caches
    echo "OpenNaga removed from $prefix. Profiles in ~/.config/opennaga were kept."
    exit 0
fi

if [ "$build" = 1 ]; then
    if [ "$(id -u)" = 0 ] && [ -z "$destdir" ]; then
        echo "Build as your user first (./install.sh --user, or cargo build --release)," >&2
        echo "then run: sudo ./install.sh --no-build" >&2
        exit 1
    fi
    (cd "$here" && cargo build --release --locked)
fi
if [ ! -x "$binary" ]; then
    echo "Missing $binary. Run cargo build --release first, or use a release archive." >&2
    exit 1
fi
if [ "$user" = 0 ] && [ -z "$destdir" ] && [ ! -w "$prefix" ] && [ "$(id -u)" != 0 ]; then
    echo "$prefix is not writable. Use sudo ./install.sh --no-build, or --user." >&2
    exit 1
fi

install -Dm755 "$binary" "$bindir/opennaga"
install -Dm644 "$here/data/$APP_ID.desktop" "$datadir/applications/$APP_ID.desktop"
install -Dm644 "$here/data/$APP_ID.metainfo.xml" "$datadir/metainfo/$APP_ID.metainfo.xml"
install -Dm644 "$here/data/icons/$APP_ID.svg" "$datadir/icons/hicolor/scalable/apps/$APP_ID.svg"
for size in $icon_sizes; do
    install -Dm644 "$here/data/icons/opennaga-$size.png" "$datadir/icons/hicolor/${size}x${size}/apps/$APP_ID.png"
done
if [ "$user" = 1 ]; then
    # The desktop entry runs "opennaga"; point it at the binary in case
    # ~/.local/bin is not on the session PATH.
    sed -i "s|^Exec=opennaga|Exec=\"$bindir/opennaga\"|" "$datadir/applications/$APP_ID.desktop"
fi
if [ "$udev" = 1 ]; then
    install -Dm644 "$here/data/60-opennaga.rules" "$udevdir/60-opennaga.rules"
    reload_udev
fi
refresh_caches

echo "OpenNaga installed to $prefix."
if [ "$udev" = 0 ] && [ -z "$destdir" ] && [ ! -f /etc/udev/rules.d/60-opennaga.rules ] && [ ! -f /usr/lib/udev/rules.d/60-opennaga.rules ] && [ ! -f /usr/local/lib/udev/rules.d/60-opennaga.rules ]; then
    echo "Next: open OpenNaga, go to Status and choose Install Access Rule, or run: sudo ./install.sh --no-build --udev"
fi
