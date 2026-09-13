use std::process::{Command, Output};

#[cfg(unix)]
use std::ffi::OsString;

#[cfg(unix)]
use std::os::unix::ffi::OsStringExt;

#[cfg(unix)]
use std::os::unix::process::CommandExt;

#[test]
fn help_names_the_api_key_variable_without_printing_its_value() {
    let secret = "lific-help-must-not-print-this-test-key";
    for args in [vec!["--help"], vec!["doctor", "--help"]] {
        let output = Command::new(env!("CARGO_BIN_EXE_lific"))
            .env("LIFIC_API_KEY", secret)
            .args(args)
            .output()
            .unwrap();
        assert!(output.status.success());
        let stdout = String::from_utf8(output.stdout).unwrap();
        let stderr = String::from_utf8(output.stderr).unwrap();
        assert!(stdout.contains("LIFIC_API_KEY"));
        assert!(!stdout.contains(secret));
        assert!(!stderr.contains(secret));
    }
}

#[cfg(unix)]
fn run_with_program_name(program: OsString, args: &[OsString]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_lific"));
    command.env_clear();
    command.arg0(program).args(args).output().unwrap()
}

#[cfg(unix)]
#[test]
fn help_sanitizes_bare_relative_and_absolute_program_names() {
    for program in [
        OsString::from_vec(b"lific\nFORGED_DIAGNOSTIC\x1b[2J".to_vec()),
        OsString::from_vec(b"bin/lific\rFORGED_DIAGNOSTIC\x1b[2J".to_vec()),
        OsString::from_vec("/tmp/lific\u{202e}FORGED_DIAGNOSTIC".as_bytes().to_vec()),
    ] {
        for args in [
            vec![OsString::from("--help")],
            vec![OsString::from("mcp"), OsString::from("--help")],
        ] {
            let output = run_with_program_name(program.clone(), &args);
            assert_eq!(output.status.code(), Some(0));
            assert!(output.stdout.is_empty() || !output.stdout.starts_with(b"\n"));
            assert!(
                !output
                    .stdout
                    .windows(b"\nFORGED_DIAGNOSTIC".len())
                    .any(|window| { window == b"\nFORGED_DIAGNOSTIC" })
            );
            assert!(!output.stdout.contains(&0x1b));
            assert!(output.stderr.is_empty());
        }
    }
}

#[cfg(unix)]
#[test]
fn invalid_utf8_argument_keeps_clap_failure_contract_and_safe_diagnostic_context() {
    let output = run_with_program_name(
        OsString::from_vec(b"lific\nFORGED_DIAGNOSTIC".to_vec()),
        &[
            OsString::from("--url"),
            OsString::from_vec(vec![0xff]),
            OsString::from("project"),
            OsString::from("list"),
        ],
    );

    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert!(!output.stderr.is_empty());
    assert!(
        !output
            .stderr
            .windows(b"\nFORGED_DIAGNOSTIC".len())
            .any(|window| { window == b"\nFORGED_DIAGNOSTIC" })
    );
}
