#!/bin/bash

RUST_LOG="neon_tracer=debug,solana_runtime::dumper_db=debug" RUST_BACKTRACE=1 ./target/release/neon-tracer 
