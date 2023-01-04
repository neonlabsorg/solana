#!/bin/bash

set ${NEON_EVM_REVISION:=e1671c0e1709cff1ee75eff499c3e0311c4cb665}

echo "Neon Validator revision=${BUILDKITE_COMMIT}"
echo "Neon EVM revision=${NEON_EVM_REVISION}"
