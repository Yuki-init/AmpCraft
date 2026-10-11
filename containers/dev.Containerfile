# Development image for building and testing the player.
# Build with scripts/build-image.sh, which passes RUST_VERSION from rust-toolchain.toml.
FROM docker.io/library/debian:trixie-slim

ARG RUST_VERSION
ARG DEBIAN_FRONTEND=noninteractive

RUN apt-get update && apt-get install -y --no-install-recommends \
        ca-certificates curl git build-essential clang pkg-config \
        libavcodec-dev libavformat-dev libavutil-dev libswresample-dev libswscale-dev \
        libavfilter-dev libavdevice-dev ffmpeg \
        libasound2-dev libasound2-plugins alsa-utils \
        libx11-dev libxcb1-dev libxkbcommon-dev libxkbcommon-x11-dev libxrandr-dev libxext-dev \
        libxcursor-dev libxi-dev libxcb-shape0-dev libxcb-render0-dev libxcb-xfixes0-dev \
        xvfb xauth openbox xdotool x11-utils imagemagick \
        libsqlite3-dev \
        fontconfig fonts-dejavu-core fonts-noto-cjk dbus-daemon \
    && rm -rf /var/lib/apt/lists/*

# Route the ALSA default device to the null plugin so audio tests never touch hardware.
RUN printf 'pcm.!default { type null }\nctl.!default { type hw card 0 }\n' > /etc/asound.conf

ENV RUSTUP_HOME=/usr/local/rustup \
    CARGO_HOME=/usr/local/cargo \
    PATH=/usr/local/cargo/bin:$PATH

RUN test -n "$RUST_VERSION" \
    && curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
       | sh -s -- -y --no-modify-path --profile minimal --default-toolchain "$RUST_VERSION" \
    && rustup component add rustfmt clippy \
    && rustup target add x86_64-pc-windows-gnu aarch64-apple-darwin \
    && cargo install --locked cargo-nextest cargo-deny \
    && rm -rf /usr/local/cargo/registry /usr/local/cargo/git \
    && chmod -R a+rwX /usr/local/rustup /usr/local/cargo

WORKDIR /work
