//! CLI tests against the real binary. Only CI-safe paths are exercised:
//! help/version, the `tools` inventory, the control-gate refusal and argument
//! validation that fails *before* any input is injected. No test here moves
//! the mouse or types — those paths are covered by the FakeBackend MCP tests.

use std::process::{Command, Output};

struct Run {
    status: i32,
    stdout: String,
    stderr: String,
}

fn run(args: &[&str], env: &[(&str, &str)]) -> Run {
    let output: Output = Command::new(env!("CARGO_BIN_EXE_panoptes"))
        .args(args)
        .envs(env.iter().copied())
        .output()
        .expect("spawn panoptes binary");
    Run {
        status: output.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

#[test]
fn version_and_help() {
    let r = run(&["--version"], &[]);
    assert_eq!(r.status, 0, "stderr: {}", r.stderr);
    assert!(r.stdout.contains("panoptes 0.1.0"), "stdout: {}", r.stdout);

    // No subcommand → help, exit 0.
    let r = run(&[], &[]);
    assert_eq!(r.status, 0);
    assert!(r.stdout.contains("MCP server"), "stdout: {}", r.stdout);
    assert!(
        r.stdout.contains("--yes-i-can-control-this-machine"),
        "help must document the control flag: {}",
        r.stdout
    );
    assert!(
        r.stdout.contains("Streamable HTTP"),
        "help must document the transport: {}",
        r.stdout
    );
}

#[test]
fn tools_inventory_locked_and_unlocked() {
    // Locked: gate flags exposed so the model can see what needs unlocking.
    let r = run(&["tools"], &[]);
    assert_eq!(r.status, 0, "stderr: {}", r.stderr);
    let v: serde_json::Value = serde_json::from_str(&r.stdout).expect("tools JSON");
    assert_eq!(v["control_authorized"], false);
    let tools = v["tools"].as_array().expect("tools array");
    assert_eq!(tools.len(), 9);
    let gated: Vec<&str> = tools
        .iter()
        .filter(|t| t["control_gated"] == true)
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        gated,
        vec![
            "mouse_position",
            "mouse_move",
            "mouse_click",
            "mouse_drag",
            "mouse_scroll",
            "key_type",
            "key_press",
        ]
    );
    for t in tools {
        assert_eq!(t["inputSchema"]["type"], "object");
    }

    // Unlocked via the environment variable; data-dir override is reflected.
    let r = run(
        &["tools", "--data-dir", "/tmp/panoptes-cli-test"],
        &[("PANOPTES_I_CAN_CONTROL", "yes")],
    );
    assert_eq!(r.status, 0);
    let v: serde_json::Value = serde_json::from_str(&r.stdout).expect("tools JSON");
    assert_eq!(v["control_authorized"], true);
    assert_eq!(v["data_dir"], "/tmp/panoptes-cli-test");

    // The explicit flag works too.
    let r = run(&["tools", "--yes-i-can-control-this-machine"], &[]);
    assert_eq!(r.status, 0);
    let v: serde_json::Value = serde_json::from_str(&r.stdout).expect("tools JSON");
    assert_eq!(v["control_authorized"], true);
}

#[test]
fn gate_refusal_exits_nonzero() {
    // Locked server refuses control tools before touching anything.
    let r = run(&["mouse-position"], &[]);
    assert_eq!(r.status, 2, "stdout: {} stderr: {}", r.stdout, r.stderr);
    assert!(
        r.stderr.contains("--yes-i-can-control-this-machine"),
        "stderr: {}",
        r.stderr
    );
    assert!(
        r.stderr.contains("PANOPTES_I_CAN_CONTROL=yes"),
        "stderr: {}",
        r.stderr
    );

    // Piped stdin JSON reaches the same funnel (and the same gate): the JSON
    // merges over the CLI args before dispatch refuses the gated tool.
    use std::io::Write;
    let mut child = Command::new(env!("CARGO_BIN_EXE_panoptes"))
        .args(["mouse-scroll", "--amount", "1"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn");
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(br#"{"axis":"vertical"}"#)
        .expect("write stdin");
    let out = child.wait_with_output().expect("wait");
    assert_eq!(out.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("--yes-i-can-control-this-machine"),
        "stderr: {stderr}"
    );
}

#[test]
fn argument_validation_fails_before_input() {
    // Invalid button name: parse fails before any real input is injected.
    let r = run(
        &[
            "mouse-click",
            "--button",
            "pinky",
            "--yes-i-can-control-this-machine",
        ],
        &[],
    );
    assert_eq!(r.status, 2, "stdout: {} stderr: {}", r.stdout, r.stderr);
    assert!(r.stderr.contains("pinky"), "stderr: {}", r.stderr);

    // Unknown tool name via the CLI funnel.
    let r = run(&["tools"], &[]);
    assert_eq!(r.status, 0);
    let bogus = run(
        &[
            "--yes-i-can-control-this-machine",
            "mouse-scroll",
            "--axis",
            "diagonal",
        ],
        &[],
    );
    assert_eq!(bogus.status, 2);
    assert!(
        bogus.stderr.contains("unknown axis 'diagonal'"),
        "stderr: {}",
        bogus.stderr
    );
}

#[test]
fn region_needs_four_numbers() {
    let r = run(&["screenshot", "--region", "1,2,3"], &[]);
    assert_eq!(r.status, 2, "clap must reject a 3-value region");
    assert!(
        r.stderr.contains("4") || r.stderr.contains("region"),
        "stderr: {}",
        r.stderr
    );
}

#[test]
fn piped_stdin_must_be_json_object() {
    use std::io::Write;
    let mut child = Command::new(env!("CARGO_BIN_EXE_panoptes"))
        .arg("screen-info")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn");
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(b"[1,2,3]")
        .expect("write stdin");
    let out = child.wait_with_output().expect("wait");
    assert_eq!(out.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("piped stdin must be a JSON object"),
        "stderr: {stderr}"
    );
}
