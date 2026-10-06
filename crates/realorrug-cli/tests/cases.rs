// SPDX-License-Identifier: Apache-2.0
//! Exercises argument dispatch through the actual operator binary, not helpers.
use std::{fs, process::Command};

#[test]
fn capture_review_and_store_dispatch_the_arguments_after_the_command_name() {
    let dir = tempfile::tempdir().unwrap();
    let request = dir.path().join("request.json");
    let capture = dir.path().join("capture.json");
    let memory = dir.path().join("memory.sqlite3");
    let review = dir.path().join("review");
    let backup = dir.path().join("backup.sqlite3");
    fs::write(&request,r#"{"id":"dispatch-test","case":{"chain":"base","address":"0x1111111111111111111111111111111111111111"},"question":"Inspect controls","wallets":[],"transactions":[],"window":null,"source":null,"thread":null}"#).unwrap();
    let run = |args: &[&std::ffi::OsStr]| {
        let result = Command::new(env!("CARGO_BIN_EXE_realorrug"))
            .args(args)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        String::from_utf8(result.stdout).unwrap()
    };
    // Base has no implicit RPC endpoint: this fixture makes no network/model call.
    run(&[
        "investigation-capture".as_ref(),
        "--request".as_ref(),
        request.as_os_str(),
        "--out".as_ref(),
        capture.as_os_str(),
    ]);
    run(&[
        "case-review".as_ref(),
        capture.as_os_str(),
        "--memory".as_ref(),
        memory.as_os_str(),
        "--out".as_ref(),
        review.as_os_str(),
    ]);
    let checkpoint = run(&[
        "case-store".as_ref(),
        "verify".as_ref(),
        "--memory".as_ref(),
        memory.as_os_str(),
    ]);
    assert_eq!(checkpoint.trim().len(), 64);
    assert_eq!(
        checkpoint,
        run(&[
            "case-store".as_ref(),
            "backup".as_ref(),
            "--memory".as_ref(),
            memory.as_os_str(),
            "--out".as_ref(),
            backup.as_os_str()
        ])
    );
    assert!(review.join("dispatch-test.review.md").is_file());
    let result: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(review.join("dispatch-test.assessment.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(result["level"], "CantTell");
    assert_eq!(result["complete"], false);
}
