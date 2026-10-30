use std::fs;
use std::path::Path;

use assert_cmd::Command;
use predicates::prelude::*;

fn fixture(root: &Path) {
    let proj = root.join("Projects/app");
    fs::create_dir_all(proj.join("node_modules/pkg")).unwrap();
    fs::write(proj.join("package.json"), "{}").unwrap();
    fs::write(
        proj.join("node_modules/pkg/big.bin"),
        vec![0u8; 3 * 1024 * 1024],
    )
    .unwrap();
}

fn cmd(config_dir: &Path) -> Command {
    let mut c = Command::cargo_bin("cleanprojects").unwrap();
    c.env("CLEANPROJECTS_CONFIG_DIR", config_dir)
        .env("NO_COLOR", "1");
    c
}

#[test]
fn help_and_doctor_work() {
    let tmp = tempfile::tempdir().unwrap();
    cmd(tmp.path())
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("scan"));
    cmd(tmp.path())
        .arg("doctor")
        .assert()
        .success()
        .stdout(predicate::str::contains("CleanProjects doctor"));
}

#[test]
fn scan_json_then_dry_run_then_real_clean() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    fixture(&home);
    let cfg = tmp.path().join("cfg");
    let root = home.join("Projects");
    let base = |c: &mut Command| {
        c.args(["--only-roots", "--root"]).arg(&root).args([
            "--no-tools",
            "--no-git",
            "--no-global",
        ]);
    };

    let mut c = cmd(&cfg);
    c.arg("scan").arg("--json");
    base(&mut c);
    let out = c.assert().success().get_output().stdout.clone();
    let json: serde_json::Value = serde_json::from_slice(&out).unwrap();
    let cands = json["candidates"].as_array().unwrap();
    assert_eq!(cands.len(), 1, "{json}");
    assert_eq!(cands[0]["category"], "project");
    let id = cands[0]["id"].as_str().unwrap().to_string();

    let mut c = cmd(&cfg);
    c.args(["clean", "--dry-run", "--yes"]);
    base(&mut c);
    c.assert()
        .success()
        .stdout(predicate::str::contains("dry-run"));
    assert!(home.join("Projects/app/node_modules").exists());

    let mut c = cmd(&cfg);
    c.args(["clean", "--yes", "--permanent", "--json", "--id", &id]);
    base(&mut c);
    c.assert()
        .success()
        .stdout(predicate::str::contains("\"ok\": true"));
    assert!(!home.join("Projects/app/node_modules").exists());

    cmd(&cfg)
        .arg("history")
        .assert()
        .success()
        .stdout(predicate::str::contains("node_modules"));
}

#[test]
fn exclusions_and_config() {
    let tmp = tempfile::tempdir().unwrap();
    let cfg = tmp.path().join("cfg");
    cmd(&cfg)
        .args(["exclude", "/tmp/some/node_modules"])
        .assert()
        .success()
        .stdout(predicate::str::contains("excluded"));
    cmd(&cfg).args(["include", "--all"]).assert().success();
    cmd(&cfg)
        .args([
            "config",
            "--set",
            "clean.use_trash=false",
            "--set",
            "ui.language=tr",
        ])
        .assert()
        .success();
    cmd(&cfg)
        .arg("config")
        .assert()
        .success()
        .stdout(predicate::str::contains("use_trash = false"))
        .stdout(predicate::str::contains("language = \"tr\""));
    cmd(&cfg)
        .args(["config", "--set", "bogus=1"])
        .assert()
        .failure();
    cmd(&cfg)
        .args(["completions", "zsh"])
        .assert()
        .success()
        .stdout(predicate::str::contains("cleanprojects"));
}

#[test]
fn clean_refuses_without_yes_when_not_a_terminal() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    fixture(&home);
    let mut c = cmd(&tmp.path().join("cfg"));
    c.args(["clean", "--only-roots", "--root"])
        .arg(home.join("Projects"))
        .args(["--no-tools", "--no-git", "--no-global"]);
    c.assert()
        .failure()
        .stderr(predicate::str::contains("--yes"));
    assert!(home.join("Projects/app/node_modules").exists());
}
