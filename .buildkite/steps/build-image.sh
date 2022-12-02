#!/bin/bash
set -euo pipefail
source .buildkite/steps/revision.sh

echo -e "\n\n\nBuilding Neon Validator..."
docker build -t neonlabsorg/neon-validator:${BUILDKITE_COMMIT} \
  --build-arg NEON_EVM_REVISION=${NEON_EVM_REVISION} \
  --build-arg SOLANA_REVISION=${SOLANA_REVISION} .

