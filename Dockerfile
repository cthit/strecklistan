##################
### BASE STAGE ###
##################
FROM rust:1.91.1 AS base

# Install build dependencies
RUN curl -L --proto '=https' --tlsv1.2 -sSf https://raw.githubusercontent.com/cargo-bins/cargo-binstall/main/install-from-binstall-release.sh | bash
RUN cargo binstall cargo-make@0.37.24 trunk@0.21.14 strip_cargo_version@0.0.3
RUN rustup target add wasm32-unknown-unknown

WORKDIR /app
RUN mkdir frontend backend common

###########################
### STRIP-VERSION STAGE ###
###########################
FROM base AS strip-version

COPY Cargo.lock Cargo.toml ./
COPY frontend/Cargo.toml ./frontend/
COPY backend/Cargo.toml ./backend/
COPY common/Cargo.toml ./common/
RUN strip_cargo_version

###################
### BUILD STAGE ###
###################
FROM base AS build

RUN cargo init --lib frontend
RUN cargo init --bin backend
RUN cargo init --lib common

COPY --from=strip-version /app/frontend/Cargo.toml /app/frontend/
COPY --from=strip-version /app/backend/Cargo.toml /app/backend/
COPY --from=strip-version /app/common/Cargo.toml /app/common/
COPY --from=strip-version /app/Cargo.toml /app/Cargo.lock /app/

WORKDIR /app/backend
RUN cargo build --release

WORKDIR /app/frontend
RUN cargo build --release --target wasm32-unknown-unknown

WORKDIR /app
COPY . .

WORKDIR /app/backend
RUN cargo build --release

WORKDIR /app/frontend
RUN trunk build --release

########################
### PRODUCTION STAGE ###
########################
FROM debian:stable-slim

# Rocket web server configuration
ENV ROCKET_ADDRESS="0.0.0.0"
ENV ROCKET_PORT="8000"

# Default to always running database migrations
ENV RUN_MIGRATIONS="true"

# Basic default configuration for database, suitable for dev.
ENV DATABASE_URL="postgres://postgres@database/strecklistan"

# Enable cache-control
ENV ENABLE_STATIC_FILE_CACHE="true"
ENV STATIC_FILES_MAX_AGE="0"

# Install dependencies
RUN apt-get update \
    && apt-get install -y libpq5 openssl \
    && apt-get autoremove && apt-get autoclean

RUN mkdir -p /www
WORKDIR /

# Copy application binary
COPY --from=build /app/target/release/strecklistan_backend /usr/local/bin/strecklistan

# Copy static web files
COPY --from=build /app/frontend/dist /www

# Copy database migrations
COPY backend/migrations /migrations

# Copy html templates
COPY backend/templates /templates

CMD ["strecklistan"]
