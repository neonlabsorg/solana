#!/bin/bash

set ${NEON_EVM_REVISION:=v0.15.2}

echo "Neon Validator revision=${BUILDKITE_COMMIT}"
echo "Neon EVM revision=${NEON_EVM_REVISION}"
