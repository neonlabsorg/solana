#!/bin/bash

set ${NEON_EVM_REVISION:=ci-v0.12.1-neon-tracer}

echo "Neon Validator revision=${BUILDKITE_COMMIT}"
echo "Neon EVM revision=${NEON_EVM_REVISION}"
