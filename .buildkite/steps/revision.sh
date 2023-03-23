#!/bin/bash

set ${NEON_EVM_REVISION:=72f95822fe1388adf0d8cbd96c0261188c0510dd}

echo "Neon Validator revision=${BUILDKITE_COMMIT}"
echo "Neon EVM revision=${NEON_EVM_REVISION}"
