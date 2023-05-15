#!/bin/bash

set ${NEON_EVM_REVISION:=21f5342f107a6b1cc0ba9bc6a54d7c69563d185a}

echo "Neon Validator revision=${BUILDKITE_COMMIT}"
echo "Neon EVM revision=${NEON_EVM_REVISION}"
