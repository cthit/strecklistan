FROM rust:1.91.1 AS build_stage

RUN apt-get update &&\
    apt-get install -y postgresql-client &&\
    apt-get autoremove && apt-get autoclean

RUN curl -L --proto '=https' --tlsv1.2 -sSf https://raw.githubusercontent.com/cargo-bins/cargo-binstall/main/install-from-binstall-release.sh | bash

RUN cargo binstall cargo-watch@8.5.3 diesel_cli@2.3.4

VOLUME /out
ENV CARGO_BUILD_TARGET_DIR /out/target

VOLUME /app
WORKDIR /app/backend
ENV ROCKET_ADDRESS 0.0.0.0
CMD sh /app/docker/start_backend.sh

