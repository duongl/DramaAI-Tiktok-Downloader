//! Windows compatibility shim for GNU dlltool.
//!
//! Rust's windows-gnu target passes `--temp-prefix <dll>:` to dlltool. On
//! Windows the colon is interpreted as an NTFS alternate-data-stream
//! separator, so dlltool fails with "CreateProcess" / "No such file or
//! directory". This shim forwards every argument except `--temp-prefix` to a
//! real dlltool placed next to this executable.

use std::env;
use std::process::{exit, Command};

fn main() {
    let exe = env::current_exe().expect("current_exe");
    let dir = exe.parent().expect("executable directory");
    let real = dir.join("dlltool-real.exe");
    let assembler = dir.join("as.exe");

    let mut args = vec![
        "--as".to_owned(),
        assembler.to_string_lossy().into_owned(),
    ];
    let mut skip_next = false;
    for arg in env::args().skip(1) {
        if skip_next {
            skip_next = false;
            continue;
        }
        if arg == "--temp-prefix" {
            skip_next = true;
            continue;
        }
        args.push(arg);
    }

    let status = Command::new(&real)
        .args(&args)
        .status()
        .unwrap_or_else(|error| panic!("failed to run {}: {error}", real.display()));

    exit(status.code().unwrap_or(1));
}
