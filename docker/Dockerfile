# Pinned build/test environment for OpenCity-rs.
# Base: official Rust image, pinned by tag AND digest (Rust 1.88.0, Debian 12 "bookworm").
FROM rust:1.88.0-slim-bookworm@sha256:38bc5a86d998772d4aec2348656ed21438d20fcdce2795b56ca434cf21430d89

# System libraries needed by macroquad/miniquad (X11, OpenGL, ALSA) plus
# Xvfb + Mesa software rendering so the game can run headless in CI for screenshots.
RUN apt-get update \
 && apt-get install -y --no-install-recommends \
      pkg-config libx11-dev libxi-dev libgl1-mesa-dev libasound2-dev \
      libxkbcommon0 libxkbcommon-x11-0 libxcursor1 libxrandr2 \
      xvfb xauth libgl1-mesa-dri \
 && rm -rf /var/lib/apt/lists/*

# Toolchain components, same pinned compiler.
RUN rustup component add clippy rustfmt

ENV CARGO_TARGET_DIR=/target
WORKDIR /src
