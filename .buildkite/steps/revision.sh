#!/bin/bash

set ${NEON_EVM_REVISION:=latest}

echo "Neon Validator revision=${BUILDKITE_COMMIT}"
echo "Neon EVM revision=${NEON_EVM_REVISION}"
