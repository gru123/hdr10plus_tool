use std::path::Path;

use anyhow::Result;
use assert_cmd::cargo;
use assert_fs::prelude::*;
use predicates::prelude::*;

const SUBCOMMAND: &str = "remove";

fn remove(cmd: &mut assert_cmd::Command, expected: &str) -> Result<()> {
    let temp = assert_fs::TempDir::new()?;
    let output_file = temp.child("hdr10plus_removed_output");

    cmd.arg("--output").arg(output_file.as_ref());
    cmd.assert().success().stderr(predicate::str::is_empty());

    output_file
        .assert(predicate::path::is_file())
        .assert(predicate::path::eq_file(Path::new(expected)));

    Ok(())
}

#[test]
fn remove_raw() -> Result<()> {
    let mut cmd = cargo::cargo_bin_cmd!();
    cmd.arg(SUBCOMMAND).arg("assets/av1_tests/regular.av1");

    remove(&mut cmd, "assets/av1_tests/regular_bl.av1")
}

#[test]
fn remove_ivf() -> Result<()> {
    let mut cmd = cargo::cargo_bin_cmd!();
    cmd.arg(SUBCOMMAND).arg("assets/av1_tests/regular.ivf");

    remove(&mut cmd, "assets/av1_tests/regular_bl.ivf")
}

/// Matroska input is written out as a raw AV1 stream
#[test]
fn remove_mkv() -> Result<()> {
    let mut cmd = cargo::cargo_bin_cmd!();
    cmd.arg(SUBCOMMAND).arg("assets/av1_tests/regular.mkv");

    remove(&mut cmd, "assets/av1_tests/regular_bl.av1")
}

#[test]
fn remove_stdin() -> Result<()> {
    let mut cmd = cargo::cargo_bin_cmd!();
    cmd.arg(SUBCOMMAND)
        .arg("-")
        .pipe_stdin("assets/av1_tests/regular.av1")?;

    remove(&mut cmd, "assets/av1_tests/regular_bl.av1")
}
