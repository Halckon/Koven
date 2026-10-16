//! A successful, bounded source program, with a deadline for compiler regressions.
use std::{
    path::Path,
    process::{Command, Output, Stdio},
    time::{Duration, Instant},
};

pub(super) fn run(executable: &Path) -> Output {
    let mut child = Command::new(executable)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("bounded numeric iteration program starts");
    let start = Instant::now();
    loop {
        if child.try_wait().expect("child status").is_some() {
            return child.wait_with_output().expect("reap bounded program");
        }
        if start.elapsed() >= Duration::from_secs(10) {
            child.kill().expect("stop only this timed out child");
            let output = child.wait_with_output().expect("reap timed out child");
            panic!("bounded iteration exceeded 10 seconds: {output:?}");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}
