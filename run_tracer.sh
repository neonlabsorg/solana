#!/bin/bash

RUST_LOG="neon_tracer=debug,solana_runtime::dumper_db=debug,solana_runtime::message_processor=debug,solana_program_runtime::invoke_context=debug" RUST_BACKTRACE=1 ./target/release/neon-tracer 
