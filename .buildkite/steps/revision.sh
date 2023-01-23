#!/bin/bash

#set ${NEON_EVM_REVISION:=latest}
set ${NEON_EVM_REVISION:=088d0cd66dcb398b09236eb54605ac06ad773c19}

echo "Neon Validator revision=${BUILDKITE_COMMIT}"
echo "Neon EVM revision=${NEON_EVM_REVISION}"
