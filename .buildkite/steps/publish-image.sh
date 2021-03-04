#!/bin/bash
set -euo pipefail

REVISION=$(git rev-parse HEAD)

docker images

docker login -u=$DHUBU -p=$DHUBP

if [[ ${BUILDKITE_BRANCH} == "master" ]]; then
    TAG=stable
elif [[ ${BUILDKITE_BRANCH} == "develop" ]]; then
    TAG=latest
else
    TAG=${BUILDKITE_BRANCH}
fi

docker pull cybercoredev/solana:${REVISION}
docker tag cybercoredev/solana:${REVISION} cybercoredev/solana:${TAG}
docker push cybercoredev/solana:${TAG}

