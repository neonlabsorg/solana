#!/bin/bash

set ${NEON_EVM_REVISION:=e92c263ddc2f277d0b2d91d71e4c882eb93f15f6}

echo "Neon Validator revision=${BUILDKITE_COMMIT}"
echo "Neon EVM revision=${NEON_EVM_REVISION}"
