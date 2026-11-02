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
    'grim: Isolated window captures excluding overlapping floating windows'
    'wl-clipboard: Native Wayland clipboard utility'
)
options=(!debug)
source=()
sha256sums=()

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
