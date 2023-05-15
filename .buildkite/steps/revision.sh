#!/bin/bash

set ${NEON_EVM_REVISION:=7b7b48e6b5d80eb90f31b8bda1d97f287a2ddefc}

echo "Neon Validator revision=${BUILDKITE_COMMIT}"
echo "Neon EVM revision=${NEON_EVM_REVISION}"
