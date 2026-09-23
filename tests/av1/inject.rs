use std::path::Path;

use anyhow::Result;
use assert_cmd::cargo;
use assert_fs::prelude::*;
use predicates::prelude::*;

const SUBCOMMAND: &str = "inject";

fn inject(input_file: &str, expected: &str) -> Result<()> {
    let mut cmd = cargo::cargo_bin_cmd!();
    let temp = assert_fs::TempDir::new()?;

    let output_file = temp.child("injected_output");

    let assert = cmd
        .arg(SUBCOMMAND)
        .arg(input_file)
        .arg("--json")
        .arg("assets/hevc_tests/regular_metadata.json")
        .arg("--output")
        .arg(output_file.as_ref())
        .assert();

    assert.success().stderr(predicate::str::is_empty());

    output_file
        .assert(predicate::path::is_file())
        .assert(predicate::path::eq_file(Path::new(expected)));

    Ok(())
}

#[test]
fn inject_raw() -> Result<()> {
    inject(
        "assets/av1_tests/regular_bl.av1",
        "assets/av1_tests/regular.av1",
    )
}

#[test]
fn inject_ivf() -> Result<()> {
    inject(
        "assets/av1_tests/regular_bl.ivf",
        "assets/av1_tests/regular.ivf",
    )
}

/// Metadata already in the stream is replaced, not doubled
#[test]
fn inject_replaces_existing() -> Result<()> {
    inject(
        "assets/av1_tests/regular.av1",
        "assets/av1_tests/regular.av1",
    )
}

#[test]
fn matroska_refused() -> Result<()> {
    let mut cmd = cargo::cargo_bin_cmd!();
    let temp = assert_fs::TempDir::new()?;

    let output_file = temp.child("injected_output.av1");

    let assert = cmd
        .arg(SUBCOMMAND)
        .arg("assets/av1_tests/regular.mkv")
        .arg("--json")
        .arg("assets/hevc_tests/regular_metadata.json")
        .arg("--output")
        .arg(output_file.as_ref())
        .assert();

    assert.failure().stderr(predicate::str::contains(
        "Must be a raw HEVC or AV1 bitstream file",
    ));

    Ok(())
}
