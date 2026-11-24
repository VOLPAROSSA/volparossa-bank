// SPDX-License-Identifier: GPL-3.0-only
//! Compatibility entry point for the original four-validator TEST trial.

#[path = "../src/bin/bank-test-authorizer/main.rs"]
mod owner;

fn main() -> std::process::ExitCode {
    owner::main()
}
