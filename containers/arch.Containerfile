# Arch Linux build image for the native Arch package (links Arch's own FFmpeg).
# Build with scripts/build-arch-package.sh, which passes RUST_VERSION from rust-toolchain.toml.
FROM docker.io/library/archlinux:latest

ARG RUST_VERSION

RUN pacman -Syu --noconfirm --needed \
        base-devel clang pkgconf ffmpeg alsa-lib rustup git \
        xorg-server-xvfb xdotool imagemagick dbus \
    && pacman -Scc --noconfirm

ENV RUSTUP_HOME=/usr/local/rustup \
    CARGO_HOME=/usr/local/cargo \
    PATH=/usr/local/cargo/bin:$PATH

RUN test -n "$RUST_VERSION" \
    && rustup toolchain install "$RUST_VERSION" --profile minimal \
    && rustup default "$RUST_VERSION" \
    && mkdir -p /usr/local/cargo/registry \
    && chmod -R a+rwX /usr/local/rustup /usr/local/cargo

WORKDIR /work
