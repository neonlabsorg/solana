ARG NEON_EVM_REVISION
ARG SOLANA_REVISION

# Use solana
FROM solanalabs/solana:${SOLANA_REVISION} AS solana

FROM neonlabsorg/evm_loader:${NEON_EVM_REVISION}
# Replace Solana in Neon-EVM image

WORKDIR /opt

COPY --from=solana \
     /usr/bin/solana \
     /usr/bin/solana-validator \
     /usr/bin/solana-keygen \
     /usr/bin/solana-faucet \
     /usr/bin/solana-genesis \
     /usr/bin/solana-run.sh \
     /usr/bin/fetch-spl.sh \
     /usr/bin/spl* \
     /opt/solana/bin/

