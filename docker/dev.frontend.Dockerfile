FROM rust:1.91.1 AS build_stage

RUN curl -L --proto '=https' --tlsv1.2 -sSf https://raw.githubusercontent.com/cargo-bins/cargo-binstall/main/install-from-binstall-release.sh | bash
RUN cargo binstall cargo-make@0.37.24 trunk@0.21.14
RUN rustup target add wasm32-unknown-unknown

VOLUME /out
ENV CARGO_BUILD_TARGET_DIR /out/target
ENV TRUNK_DIST_DIR /out/dist

VOLUME /app
WORKDIR /app/frontend
CMD trunk serve
