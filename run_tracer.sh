#!/bin/bash

RUST_LOG="neon_tracer=debug,solana_runtime::dumper_db=debug,solana_runtime::message_processor=debug,solana_program_runtime::invoke_context=debug" RUST_BACKTRACE=1 ./target/release/neon-tracer --connection-str "host=localhost dbname=solana user=solana-user port=5432 password=solana-pass" replay 3uQrW5kh6phfUNn1bc8gBdyAgnK8h1kQNotUvBwzRJ9EynqyMXTpiuu2RLDSHbe1AFqwbstQNqfLLjj7VAv3WZ7k "{ \"result\": {{{result}}}, \"logs\": {{{logs}}}, \"return\": {{{return}}} }"
