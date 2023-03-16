#!/bin/bash

set ${NEON_EVM_REVISION:=5a4fa77fd32d25022b6fa51a7651e2bced3d09a0}

echo "Neon Validator revision=${BUILDKITE_COMMIT}"
echo "Neon EVM revision=${NEON_EVM_REVISION}"
