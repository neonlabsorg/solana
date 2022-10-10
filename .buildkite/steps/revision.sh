#!/bin/bash

set ${NEON_EVM_REVISION:=ci-develop-neon-tracer}

echo "Neon Validator revision=${BUILDKITE_COMMIT}"
echo "Neon EVM revision=${NEON_EVM_REVISION}"
