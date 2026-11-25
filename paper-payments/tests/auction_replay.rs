// SPDX-License-Identifier: GPL-3.0-only
//! Independent CLI replays for the fictional order book.
use std::{
    fs,
    io::Write as _,
    path::Path,
    process::{Command, Output, Stdio},
};

const READER: &str = env!("CARGO_BIN_EXE_paper-auction");

fn sample(command: &str) -> String {
    let output = Command::new(READER).arg(command).output().unwrap();
    assert!(output.status.success());
    String::from_utf8(output.stdout).unwrap()
}

fn replay(command: &str, roster: &Path, orders: &str) -> Output {
    let mut reader = Command::new(READER)
        .arg(command)
        .arg(roster)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    reader
        .stdin
        .take()
        .unwrap()
        .write_all(orders.as_bytes())
        .unwrap();
    reader.wait_with_output().unwrap()
}

#[test]
fn two_readers_agree_on_every_fill_and_refuse_a_changed_order() {
    let private = tempfile::tempdir().unwrap();
    let roster = private.path().join("roster");
    fs::write(&roster, sample("sample-roster")).unwrap();
    let orders = sample("sample-orders");
    let quote = replay("verify", &roster, &orders);
    assert!(quote.status.success());
    assert_eq!(quote.stdout, b"TEST-SHARE: 7 units at 10 TEST per unit\n");
    let reader = replay("allocate", &roster, &orders);
    let reversed = orders.lines().rev().collect::<Vec<_>>().join("\n");
    let other = replay("allocate", &roster, &reversed);
    assert!(reader.status.success() && other.status.success());
    assert_eq!(reader.stdout, other.stdout);

    let keys: Vec<_> = orders
        .lines()
        .map(|line| line.split_once(' ').unwrap().0)
        .collect();
    let mut fills = [
        format!("{} buy 7", keys[0]),
        format!("{} sell 5", keys[2]),
        format!("{} sell 2", keys[3]),
    ];
    fills.sort();
    let expected = format!(
        "TEST-SHARE: 7 units at 10 TEST per unit\n{}\n",
        fills.join("\n")
    );
    assert_eq!(reader.stdout, expected.as_bytes());

    let invalid = replay(
        "allocate",
        &roster,
        &orders.replace("sell 4 10", "sell 4 9"),
    );
    assert!(!invalid.status.success());
    assert!(invalid.stdout.is_empty());
    let empty = replay("allocate", &roster, "");
    assert!(empty.status.success());
    assert_eq!(empty.stdout, b"TEST-SHARE: no crossing orders\n");
}
