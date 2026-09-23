use std::path::Path;

use anyhow::Result;
use assert_cmd::cargo;
use assert_fs::prelude::*;
use predicates::prelude::*;

const SUBCOMMAND: &str = "extract";

/// The SceneInfo of a JSON file, the per frame metadata
fn scene_info(path: &Path) -> Result<serde_json::Value> {
    let json: serde_json::Value = serde_json::from_reader(std::fs::File::open(path)?)?;
    Ok(json["SceneInfo"].clone())
}

fn assert_extracts_regular_metadata(cmd: &mut assert_cmd::Command) -> Result<()> {
    let temp = assert_fs::TempDir::new()?;
    let output_json = temp.child("metadata.json");

    cmd.arg("--output").arg(output_json.as_ref());
    cmd.assert().success().stderr(predicate::str::is_empty());

    output_json.assert(predicate::path::is_file());

    let expected = scene_info(Path::new("assets/hevc_tests/regular_metadata.json"))?;
    assert_eq!(scene_info(output_json.path())?, expected);

    Ok(())
}

#[test]
fn extract_raw() -> Result<()> {
    let mut cmd = cargo::cargo_bin_cmd!();
    cmd.arg(SUBCOMMAND).arg("assets/av1_tests/regular.av1");

    assert_extracts_regular_metadata(&mut cmd)
}

#[test]
fn extract_ivf() -> Result<()> {
    let mut cmd = cargo::cargo_bin_cmd!();
    cmd.arg(SUBCOMMAND).arg("assets/av1_tests/regular.ivf");

    assert_extracts_regular_metadata(&mut cmd)
}

#[test]
fn extract_mkv() -> Result<()> {
    let mut cmd = cargo::cargo_bin_cmd!();
    cmd.arg(SUBCOMMAND).arg("assets/av1_tests/regular.mkv");

    assert_extracts_regular_metadata(&mut cmd)
}

#[test]
fn extract_stdin() -> Result<()> {
    let mut cmd = cargo::cargo_bin_cmd!();
    cmd.arg(SUBCOMMAND)
        .arg("-")
        .pipe_stdin("assets/av1_tests/regular.av1")?;

    assert_extracts_regular_metadata(&mut cmd)
}

#[test]
fn verify() -> Result<()> {
    let mut cmd = cargo::cargo_bin_cmd!();
    let assert = cmd
        .arg("--verify")
        .arg(SUBCOMMAND)
        .arg("assets/av1_tests/regular.av1")
        .assert();

    // Finding metadata is what --verify asks about: a success, as for HEVC
    assert.success().stdout(predicate::str::contains(
        "Dynamic HDR10+ metadata detected.",
    ));

    Ok(())
}

#[test]
fn no_metadata() -> Result<()> {
    let mut cmd = cargo::cargo_bin_cmd!();
    let temp = assert_fs::TempDir::new()?;
    let output_json = temp.child("metadata.json");

    let assert = cmd
        .arg(SUBCOMMAND)
        .arg("assets/av1_tests/regular_bl.av1")
        .arg("--output")
        .arg(output_json.as_ref())
        .assert();

    assert.failure().stderr(predicate::str::contains(
        "File doesn't contain dynamic metadata",
    ));

    output_json.assert(predicate::path::missing());

    Ok(())
}
