// Copyright (c) 2026 mba-x86. Released under the MIT License (see LICENSE).
// Hide the console window in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    pk_lib::run();
}
