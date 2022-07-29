FROM solanalabs/rust:latest AS builder

COPY . /opt
WORKDIR /opt
RUN cargo build --release \
    --bin solana \
    --bin solana-validator \
    --bin solana-faucet \
    --bin solana-genesis \
    --bin solana-keygen \
    --lib


FROM ubuntu:20.04

RUN apt-get update \
    && DEBIAN_FRONTEND=noninteractive apt-get -y install openssl ca-certificates curl \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /opt/target/release/solana \
                    /opt/target/release/solana-faucet \
                    /opt/target/release/solana-keygen \
                    /opt/target/release/solana-validator \
                    /opt/target/release/solana-genesis \
                    /opt/target/release/libsolana_geyser_plugin_postgres.so \
                    /opt/solana/bin/

COPY --from=builder /opt/scripts/run.sh /opt/fetch-spl.sh /opt/solana/
COPY accountsdb-plugin-config.json /opt

ENV PATH /opt/solana/bin/:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin
WORKDIR /opt/solana
EXPOSE 8899/tcp 9900/tcp 8900/tcp 8003/udp
ENTRYPOINT [ "./run.sh" ]
