#!/bin/bash

set ${NEON_EVM_REVISION:=a482f6467c5898eab786ffa03b1ae75dcd981d3c}

echo "Neon Validator revision=${BUILDKITE_COMMIT}"
echo "Neon EVM revision=${NEON_EVM_REVISION}"
