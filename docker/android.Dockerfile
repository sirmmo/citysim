# Pinned Android build environment.
# Base: macroquad's cargo-quad-apk image (Android SDK 31, NDK r25, JDK, cargo-quad-apk),
# pinned by digest. It ships Rust 1.79, so the workspace toolchain (1.88.0) is added on top.
FROM notfl3/cargo-apk@sha256:6b70dabc44ef283b9533ff5f49ccd246579ee1b4204691bf5ab322c5846dbc0b

RUN rustup toolchain install 1.88.0 --profile minimal \
      --target aarch64-linux-android --target armv7-linux-androideabi \
 && rustup default 1.88.0

WORKDIR /src
