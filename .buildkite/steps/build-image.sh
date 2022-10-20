#!/bin/bash
set -euo pipefail
source .buildkite/steps/revision.sh

echo -e "\n\n\nBuilding Neon Validator..."
docker build -t neonlabsorg/neon-validator:${BUILDKITE_COMMIT} .

#echo -r "\n\n\nBuilding Accounts DB..."
#docker build -t neonlabsorg/neon-accountsdb:${BUILDKITE_COMMIT} ./neon-dumper-plugin/db
