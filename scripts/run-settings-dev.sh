#!/usr/bin/env bash
set -e

glib-compile-schemas data

cargo build --bin wiretray

GSETTINGS_SCHEMA_DIR="$PWD/data" cargo run --bin wiretray-settings