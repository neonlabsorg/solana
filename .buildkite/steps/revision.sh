#!/bin/bash

set ${NEON_EVM_REVISION:=32bf97c246d9baf4a678e658e4b62b65d120100a}

echo "Neon Validator revision=${BUILDKITE_COMMIT}"
echo "Neon EVM revision=${NEON_EVM_REVISION}"
