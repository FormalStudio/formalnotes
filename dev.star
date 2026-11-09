# rustup's proxies act on the name they run as and install the toolchain rust-toolchain.toml pins.
RUST = ["cargo", "rustc", "rustdoc", "rustfmt", "cargo-fmt", "cargo-clippy", "clippy-driver"]

# gpui-pre links libxkbcommon and freetype at build time and dlopens wayland, vulkan,
# fontconfig and X11 at runtime.
LIBS = ["xkbcommon", "wayland-client", "wayland-protocols", "vulkan", "fontconfig", "freetype2",
        "xcb", "x11", "xcursor", "xi", "xrandr", "egl", "alsa"]

dev(
    packages = [pkg("rustup", links = {name: "rustup" for name in RUST}), pkg("cargo-nextest"), pkg("just")],
    libs = [lib(n) for n in LIBS] if platform.is_linux else [],
    bins = [bin(n) for n in ["pkg-config", "gcc", "grim", "wl-copy"]] if platform.is_linux else [],
)
