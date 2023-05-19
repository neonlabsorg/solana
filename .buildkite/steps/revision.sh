#!/bin/bash

set ${NEON_EVM_REVISION:=e9958884baedfd0e029e823e92e7c6ff2ccb52c2}

echo "Neon Validator revision=${BUILDKITE_COMMIT}"
echo "Neon EVM revision=${NEON_EVM_REVISION}"
