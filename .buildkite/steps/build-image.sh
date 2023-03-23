#!/bin/bash
set -euo pipefail
source .buildkite/steps/revision.sh

docker pull neonlabsorg/evm_loader:${NEON_EVM_REVISION}

echo -e "\n\n\nBuilding Neon Validator..."
docker build -t neonlabsorg/neon-validator:${BUILDKITE_COMMIT} \
  --build-arg NEON_EVM_REVISION=${NEON_EVM_REVISION} \
  --build-arg NEON_TRACER_REVISION=${BUILDKITE_COMMIT} .

echo -r "\n\n\nBuilding Accounts DB..."
docker build -t neonlabsorg/neon-accountsdb:${BUILDKITE_COMMIT} ./neon-dumper-plugin/db
