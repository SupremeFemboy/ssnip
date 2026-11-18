# Maintainer: Supreme Femboy
pkgname=ssnip
pkgver=0.1.0
pkgrel=1
pkgdesc="Fast and minimalistic Wayland screenshot utility written in Rust"
arch=('x86_64')
url="https://github.com/RustBlyat/ssnip"
license=('MIT' 'Apache-2.0')
depends=('gcc-libs' 'glibc' 'wayland' 'libwebp')
makedepends=('cargo')
optdepends=(
    'wl-clipboard: Native Wayland clipboard utility'
)
options=(!debug)
source=()
sha256sums=()

# Keep makepkg's srcdir/pkgdir under target/makepkg so running makepkg from the
# repository root never deletes or overwrites the Rust src/ directory.
if [[ -n "${startdir:-}" && ( -z "${BUILDDIR:-}" || "$BUILDDIR" -ef "$startdir" ) ]]; then
    BUILDDIR="$startdir/target/makepkg"
fi

pkgver() {
    cd "$startdir"
    if git rev-parse --git-dir >/dev/null 2>&1 && [ -n "$(git rev-parse --short HEAD 2>/dev/null)" ]; then
        if git describe --long --tags --always >/dev/null 2>&1; then
            git describe --long --tags --always | sed 's/\([^-]*-g\)/r\1/;s/-/./g'
        else
            printf "r%s.g%s" "$(git rev-list --count HEAD)" "$(git rev-parse --short HEAD)"
        fi
    else
        echo "0.1.0"
    fi
}

prepare() {
    cd "$startdir"
    export RUSTUP_TOOLCHAIN=stable
    cargo fetch --target "$(rustc -vV | sed -n 's/host: //p')"
}

build() {
    cd "$startdir"
    export RUSTUP_TOOLCHAIN=stable
    export CARGO_TARGET_DIR=target
    cargo build --release
}

check() {
    cd "$startdir"
    export RUSTUP_TOOLCHAIN=stable
    export CARGO_TARGET_DIR=target
    cargo test --release
}

package() {
    cd "$startdir"
    install -Dm755 "target/release/$pkgname" "$pkgdir/usr/bin/$pkgname"
    install -Dm644 "README.md" "$pkgdir/usr/share/doc/$pkgname/README.md"
}
