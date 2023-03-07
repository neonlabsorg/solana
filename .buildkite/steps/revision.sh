#!/bin/bash

set ${NEON_EVM_REVISION:=a0c93d341d278d08d4f9505b01087b9f89a12b44}

echo "Neon Validator revision=${BUILDKITE_COMMIT}"
echo "Neon EVM revision=${NEON_EVM_REVISION}"
