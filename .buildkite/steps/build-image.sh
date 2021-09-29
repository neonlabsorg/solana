#!/bin/bash
set -euo pipefail

REVISION=$(git rev-parse HEAD)

docker build -t neonlabsorg/solana:${REVISION} .
