# syntax=docker/dockerfile:1

# Hireling production image (E11: spec FR-1/FR-2/FR-3, design D1/D2/D3).
#
# Three build stages — no official image carries both toolchains (D1):
#
#   web     node:22-bookworm-slim  -> builds web/dist (npm ci, npm run build;
#                                    tag satisfies web/package.json engines >=22)
#   build   rust:1.99.0-bookworm   -> release binary + healthcheck probe
#                                    (tag matches rust-toolchain.toml's exact
#                                    pin, so rustup does not re-download the
#                                    toolchain on cold builds)
#   runtime debian:bookworm-slim   -> binary + bundle + CA certificates,
#                                    non-root, stateless
#
# Bench note (design D5): the seats have no Docker daemon. This file's
# bench-side verification is line-by-line review against FR-1/FR-2/FR-3;
# the first deploy on Mimir is the build gate.

# Base images are pinned tag@sha256 (grizzly-gate's semgrep leg fail-closes
# on unpinned FROMs; digests resolved off the Docker Hub registry API on
# 2026-10-14 — multi-arch index digests, platform resolved at pull time on
# the amd64 host). Official images rebuild tags periodically, so a digest
# moves under security updates: bump deliberately, in its own PR, re-resolve
# the digest the same way — same policy as the rust-toolchain.toml pin and
# the gate image digest.

FROM node:22-bookworm-slim@sha256:c3de60bf2f9dd0ac6370e6117950ff62d6e339527e7472301c9c78a017978392 AS web
WORKDIR /src/web
# Manifests first: the npm ci layer caches independently of app source.
COPY web/package.json web/package-lock.json ./
RUN --mount=type=cache,target=/root/.npm \
    npm ci
COPY web/ ./
RUN npm run build

FROM rust:1.99.0-bookworm@sha256:114c7a4425406451c2866b6aafe69fe29b1b298832db1277d411ac73c82d04d6 AS build
WORKDIR /src
# Manifests and workspace layout first: the dependency compile is the long
# pole, and its cache mounts survive source edits (D2). `data/` rides along
# because src/import/mod.rs include_str!s data/seed/condition-tiers.json at
# compile time; `migrations/` because db.rs embeds sqlx::migrate!("./migrations").
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY engine ./engine
COPY migrations ./migrations
COPY data ./data
COPY src ./src
# --locked: Cargo.lock is a build input — drift fails the build on Mimir
# instead of quietly resolving new versions. Cache mounts per D2; the target
# mount is not part of the layer, so the binaries are copied out in the same
# RUN.
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --release --locked --bins \
    && mkdir /out \
    && cp target/release/hireling target/release/healthcheck /out/

FROM debian:bookworm-slim@sha256:7c7b2c966bc9ee8cedfeef67e0e279108992c77681fa595db4a9d65c06ccc587
# CA certificates: OIDC (Authentik) and the corpus import (GitHub releases)
# make TLS calls from inside the container. Nothing else is installed (D1:
# no curl/wget in the runtime — the shipped probe binary is the healthcheck).
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && apt-get clean \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --user-group hireling
COPY --from=build /out/hireling /usr/local/bin/hireling
COPY --from=build /out/healthcheck /usr/local/bin/healthcheck
COPY --from=web /src/web/dist /app/web/dist
# WORKDIR /app puts the default HIRELING_STATIC_DIR ("web/dist",
# src/config.rs) at /app/web/dist — the bundle needs no env override.
WORKDIR /app
USER hireling
EXPOSE 3000
# Exec form: the binary is PID 1 so Docker's SIGTERM reaches the existing
# drain path (WS bye + Close 1001, HTTP drain, 10s window — src/http.rs).
# The runbook sets stop_grace_period=15s above that window (FR-3, D3).
ENTRYPOINT ["/usr/local/bin/hireling"]
# Timings fixed in design D1. start-period 60s covers boot-time migrate +
# bind (db.rs::connect_and_migrate completes before http.rs::serve binds);
# 30s interval x 3 retries flags a wedged process within ~90s without
# flapping on a transiently busy box. The runbook records the observed
# first-boot time on Mimir and tunes start-period toward it.
HEALTHCHECK --interval=30s --timeout=5s --start-period=60s --retries=3 CMD ["/usr/local/bin/healthcheck"]
