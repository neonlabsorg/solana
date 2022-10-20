#!/bin/bash
set -euo pipefail
source .buildkite/steps/revision.sh

docker images

docker login -u=$DHUBU -p=$DHUBP

if [[ ${BUILDKITE_BRANCH} == "master" ]]; then
    TAG=stable
elif [[ ${BUILDKITE_BRANCH} == "develop" ]]; then
    TAG=latest
else
    TAG=${BUILDKITE_BRANCH}
fi

docker pull neonlabsorg/neon-validator:${BUILDKITE_COMMIT}
docker tag neonlabsorg/neon-validator:${BUILDKITE_COMMIT} neonlabsorg/neon-validator:${TAG}
docker push neonlabsorg/neon-validator:${TAG}

