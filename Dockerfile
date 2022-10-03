
FROM solanalabs/rust:latest AS builder
#Build Solana and Dumper-plugin


COPY . /opt
WORKDIR /opt
RUN cargo build --release \
    --bin solana \
    --bin solana-validator \
    --bin solana-faucet \
    --bin solana-genesis \
    --bin solana-keygen \
    --lib

# Download and build spl-token
FROM builder AS spl-token-builder
ADD http://github.com/solana-labs/solana-program-library/archive/refs/tags/token-cli-v2.0.14.tar.gz /opt/
RUN tar -xvf /opt/token-cli-v2.0.14.tar.gz && \
    cd /opt/solana-program-library-token-cli-v2.0.14/token/cli && \
    cargo build --release && \
    cp /opt/solana-program-library-token-cli-v2.0.14/target/release/spl-token /opt/


FROM ubuntu:20.04

RUN apt-get update \
    && DEBIAN_FRONTEND=noninteractive apt-get -y install openssl ca-certificates curl \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /opt/target/release/solana \
                    /opt/target/release/solana-faucet \
                    /opt/target/release/solana-keygen \
                    /opt/target/release/solana-validator \
                    /opt/target/release/solana-genesis \
                    /opt/solana/bin/

COPY --from=builder /opt/scripts/run.sh /opt/solana/bin/solana-run.sh
COPY --from=builder /opt/fetch-spl.sh /opt/solana/bin/

COPY --from=spl-token-builder /opt/spl-token /usr/bin/

COPY ./accountsdb-plugin-config.json /opt/
