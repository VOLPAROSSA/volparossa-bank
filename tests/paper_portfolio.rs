// SPDX-License-Identifier: GPL-3.0-only
use std::io::Write;
use std::process::{Command, Output, Stdio};

const HEADER: &str = "SNAPSHOT invented-fixture-v1 500 800\n";

fn quote(args: &[&str], snapshot: &str) -> Output {
    raw_quote(args, &format!("{HEADER}{snapshot}"))
}

fn raw_quote(args: &[&str], snapshot: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_volparossa-bank-research"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(snapshot.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

const ARGS: &[&str] = &["100", "1000", "100", "1", "5", "EUR"];

#[test]
fn plans_a_capped_contribution_and_keeps_the_rounding_residue() {
    let output = quote(
        ARGS,
        "TEST-A 100000 30000 900 10 100 13 3\nTEST-B 100000 10000 950 8 200 0 0\nTEST-C -1 -1 950 3 100 0 0\nTEST-D 0 1 950 9 100 0 0\n",
    );
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "Paper contribution: 100 TEST\nSource invented-fixture-v1, period 500..800, valuation 1000, metric currency EUR\nMetric policy: roic-ppm-times-fcf-yield-ppm-v1\nasset weight whole_units cost_TEST\nTEST-A 3/4 3 30\nTEST-B 1/4 3 24\nTEST-D 0/1 0 0\nTEST-C excluded (BothNegative)\nUnallocated: 46 TEST\n"
    );
}

#[test]
fn invalid_rows_refuse_the_entire_report() {
    for row in [
        "TEST-B 1 1 899 1 100 0 0",
        "TEST-B 1 1 1001 1 100 0 0",
        "TEST-A 1 1 950 1 100 0 0",
        "REAL 1 1 950 1 100 0 0",
        "TEST-B 1 1 950 0 100 0 0",
        "TEST-B 1 1 950 1 100 ? 0",
    ] {
        let output = quote(ARGS, &format!("TEST-A 2 2 950 1 100 0 0\n{row}\n"));
        assert!(!output.status.success(), "{row}");
        assert!(output.stdout.is_empty());
    }
}

#[test]
fn no_portfolio_or_no_headroom_preserves_the_full_budget() {
    for snapshot in ["", "TEST-A 1 0 1000 2 100 0 0", "TEST-A 1 1 950 1 100 19 1"] {
        let output = quote(ARGS, snapshot);
        assert!(output.status.success());
        assert!(
            String::from_utf8(output.stdout)
                .unwrap()
                .ends_with("Unallocated: 100 TEST\n")
        );
    }
}

#[test]
fn missing_or_inconsistent_provenance_refuses_the_report() {
    for snapshot in [
        "TEST-A 1 1 950 1 100 0 0",
        "SNAPSHOT fixture 600 400",
        "SNAPSHOT fixture 500 1001",
        "SNAPSHOT fixture 500 960\nTEST-A 1 1 950 1 100 0 0",
    ] {
        let output = raw_quote(ARGS, snapshot);
        assert!(!output.status.success(), "{snapshot}");
        assert!(output.stdout.is_empty());
    }
}
