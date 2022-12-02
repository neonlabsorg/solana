#!/bin/bash

set ${NEON_EVM_REVISION:=ci-v0.14.0-neon-tracer}
set ${SOLANA_REVISION:=v1.13.4}

echo "Neon Validator revision=${BUILDKITE_COMMIT}"
echo "Neon EVM revision=${NEON_EVM_REVISION}"
echo "Solana revision=${SOLANA_REVISION}"
