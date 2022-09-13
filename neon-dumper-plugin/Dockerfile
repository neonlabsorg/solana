# syntax=docker/dockerfile:1
FROM solanalabs/rust:1.62.0 AS builder

COPY . /opt/

WORKDIR /opt/

RUN cargo build --release

FROM scratch

COPY --from=builder \
    /opt/target/release/libneon_dumper_plugin.so \
    /opt/
