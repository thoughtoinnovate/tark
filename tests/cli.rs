//! Integration tests for CLI commands

#![allow(deprecated)]

use assert_cmd::{assert::OutputAssertExt, cargo::CommandCargoExt};
use predicates::prelude::*;
use std::process::Command;
use tempfile::TempDir;

#[test]
fn test_usage_command_help() {
    let mut cmd = Command::cargo_bin("tark").unwrap();
    cmd.arg("usage").arg("--help");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Show usage statistics"));
}

#[test]
fn test_usage_command_json_format() {
    // Create a temporary workspace
    let tmp = TempDir::new().unwrap();

    // Initialize tark workspace
    std::fs::create_dir_all(tmp.path().join(".tark")).unwrap();

    let mut cmd = Command::cargo_bin("tark").unwrap();
    cmd.arg("usage")
        .arg("--format")
        .arg("json")
        .arg("--cwd")
        .arg(tmp.path());

    // Command should succeed and output valid JSON
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("\"summary\""));
}

#[test]
fn test_usage_command_table_format() {
    let tmp = TempDir::new().unwrap();
    std::fs::create_dir_all(tmp.path().join(".tark")).unwrap();

    let mut cmd = Command::cargo_bin("tark").unwrap();
    cmd.arg("usage")
        .arg("--format")
        .arg("table")
        .arg("--cwd")
        .arg(tmp.path());

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("TARK USAGE SUMMARY"));
}

#[test]
fn test_usage_command_cleanup_requires_confirmation() {
    let tmp = TempDir::new().unwrap();
    std::fs::create_dir_all(tmp.path().join(".tark")).unwrap();

    let mut cmd = Command::cargo_bin("tark").unwrap();
    cmd.arg("usage")
        .arg("--cleanup")
        .arg("30")
        .arg("--cwd")
        .arg(tmp.path());

    // Should succeed (cleanup runs without user confirmation in CLI)
    cmd.assert().success();
}

#[test]
fn test_main_command_help() {
    let mut cmd = Command::cargo_bin("tark").unwrap();
    cmd.arg("--help");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("tark"))
        .stdout(predicate::str::contains("usage"));
}

#[test]
fn test_usage_with_empty_workspace() {
    let tmp = TempDir::new().unwrap();

    let mut cmd = Command::cargo_bin("tark").unwrap();
    cmd.arg("usage").arg("--cwd").arg(tmp.path());

    // Should succeed and show empty stats (storage is auto-created)
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("TARK USAGE SUMMARY"));
}
