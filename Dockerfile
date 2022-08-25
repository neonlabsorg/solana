FROM solanalabs/rust:latest AS builder

COPY . /opt
WORKDIR /opt
RUN cargo build --release \
    --bin solana \
    --bin solana-validator \
    --bin solana-faucet \
    --bin solana-genesis \
    --bin solana-keygen


FROM ubuntu:20.04

RUN apt-get update \
    && DEBIAN_FRONTEND=noninteractive apt-get -y install openssl ca-certificates curl \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /opt/target/release/solana \
                    /opt/target/release/solana-faucet \
                    /opt/target/release/solana-keygen \
                    /opt/target/release/solana-validator \
                    /opt/target/release/solana-genesis \
                    /usr/bin/

COPY --from=builder /opt/scripts/run.sh /usr/bin/solana-run.sh
COPY --from=builder /opt/fetch-spl.sh /usr/bin/

ENV PATH /usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin
WORKDIR /usr/bin
EXPOSE 8899/tcp 9900/tcp 8900/tcp 8003/udp
ENTRYPOINT [ "./solana-run.sh" ]
