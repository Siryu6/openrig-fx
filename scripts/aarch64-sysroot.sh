#!/bin/sh
# Make an aarch64 glibc sysroot for cross-linking the plugins with clang
# and lld, without a cross GCC installed: the arm64 cross packages of
# Debian bookworm (glibc 2.36, older than the OpenRig image's 2.41),
# copied out of a container. Needs podman or docker.
#
#   scripts/aarch64-sysroot.sh target/aarch64-sysroot
#   eval "$(scripts/aarch64-sysroot.sh --env target/aarch64-sysroot)"
#   cargo xtask bundle --target aarch64-unknown-linux-gnu
set -eu

env_only=false
if [ "${1:-}" = "--env" ]; then
	env_only=true
	shift
fi
sysroot=${1:?usage: aarch64-sysroot.sh [--env] DIR}

if ! $env_only; then
	engine=$(command -v podman || command -v docker || true)
	[ -n "$engine" ] || { echo "needs podman or docker" >&2; exit 1; }
	mkdir -p "$sysroot"
	"$engine" run --rm -v "$(cd "$sysroot" && pwd)":/out:Z debian:bookworm sh -euc '
		apt-get update -qq
		apt-get install -qq -y --no-install-recommends libc6-dev-arm64-cross libgcc-12-dev-arm64-cross >/dev/null
		mkdir -p /out/usr/lib
		cp -a /usr/aarch64-linux-gnu /out/usr/
		cp -a /usr/lib/gcc-cross /out/usr/lib/
		chown -R "$(stat -c %u:%g /out)" /out/usr
	'
	echo "sysroot in $sysroot" >&2
fi

sysroot=$(cd "$sysroot" && pwd)
cat <<ENV
export CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=clang
export CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_RUSTFLAGS="-C link-arg=--target=aarch64-linux-gnu -C link-arg=--sysroot=$sysroot -C link-arg=--gcc-toolchain=$sysroot/usr -C link-arg=-fuse-ld=lld"
ENV
