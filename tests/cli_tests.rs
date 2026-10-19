use assert_cmd::Command;
use predicates::prelude::*;
use serde_json::Value;
use std::net::TcpListener as StdTcpListener;

fn netwatch_bin() -> Command {
    Command::cargo_bin("netwatch").unwrap()
}

#[test]
fn cli_monitor_ping_success() {
    let mut cmd = netwatch_bin();
    cmd.args(["monitor", "ping", "127.0.0.1"])
        .assert()
        .success()
        .stdout(predicate::str::contains("protocol"))
        .stdout(predicate::str::contains("icmp"));

    let output = cmd.output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let json: Value = serde_json::from_str(&stdout).unwrap();

    assert_eq!(json["protocol"], "icmp");
    assert_eq!(json["target"], "127.0.0.1");
    assert!(json["success"].as_bool().unwrap_or(false));
    assert!(json["latency_ms"].is_number());
    assert_eq!(json["evidence"], "icmp");
    assert_eq!(json["timeout_ms"], 2000);
}

#[test]
fn cli_monitor_tcp_success() {
    let listener = StdTcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    listener.set_nonblocking(true).unwrap();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(500));
        let _ = listener.accept();
    });

    let mut cmd = netwatch_bin();
    cmd.args(["monitor", "tcp", "127.0.0.1", &port.to_string()])
        .assert()
        .success()
        .stdout(predicate::str::contains("protocol"))
        .stdout(predicate::str::contains("tcp"));

    let output = cmd.output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let json: Value = serde_json::from_str(&stdout).unwrap();

    assert_eq!(json["protocol"], "tcp");
    assert_eq!(json["target"], format!("127.0.0.1:{}", port));
    assert!(json["success"].as_bool().unwrap_or(false));
    assert!(json["latency_ms"].is_number());
    assert_eq!(json["evidence"], "tcp_probe");
    assert_eq!(json["timeout_ms"], 2000);
}

#[test]
fn cli_monitor_tcp_failure() {
    let mut cmd = netwatch_bin();
    cmd.args(["monitor", "tcp", "127.0.0.1", "54321"])
        .assert()
        .success();

    let output = cmd.output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let json: Value = serde_json::from_str(&stdout).unwrap();

    assert_eq!(json["protocol"], "tcp");
    assert_eq!(json["target"], "127.0.0.1:54321");
    assert_eq!(json["success"], false);
    assert!(json["latency_ms"].is_null());
    assert!(json["error"].is_string());
    assert_eq!(json["evidence"], "tcp_probe");
    assert_eq!(json["timeout_ms"], 2000);
}

#[test]
fn cli_discover_local() {
    let mut cmd = netwatch_bin();
    cmd.args(["discover", "local"])
        .assert()
        .success()
        .stdout(predicates::str::contains("Discovered"))
        .stdout(predicates::str::contains("interfaces"))
        .stdout(predicates::str::contains("routes"));

    let output = cmd.output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();

    assert!(stdout.contains("name:"));
    assert!(stdout.contains("state:"));
    assert!(stdout.contains("ip4:"));
    assert!(stdout.contains("dest:"));
    assert!(stdout.contains("via:"));
    assert!(stdout.contains("dev:"));
}

#[test]
fn cli_help() {
    let mut cmd = netwatch_bin();
    cmd.arg("--help")
        .assert()
        .success()
        .stdout(predicates::str::contains(
            "Netwatch network monitoring tool",
        ));

    let mut cmd = netwatch_bin();
    cmd.args(["monitor", "--help"])
        .assert()
        .success()
        .stdout(predicates::str::contains("monitor"));

    let mut cmd = netwatch_bin();
    cmd.args(["discover", "--help"])
        .assert()
        .success()
        .stdout(predicates::str::contains("discover"));
}

#[test]
fn cli_version() {
    let mut cmd = netwatch_bin();
    cmd.arg("--version")
        .assert()
        .success()
        .stdout(predicates::str::contains("netwatch"));
}
