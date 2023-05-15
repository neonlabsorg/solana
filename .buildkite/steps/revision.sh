#!/bin/bash

set ${NEON_EVM_REVISION:=a3771afc8bf97bd08d9c0bef4fbf6feeb37b57f5}

echo "Neon Validator revision=${BUILDKITE_COMMIT}"
echo "Neon EVM revision=${NEON_EVM_REVISION}"
