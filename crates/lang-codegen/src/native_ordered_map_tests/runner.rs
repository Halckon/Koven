//! Bounded native fixture runs; Clang follows the existing synchronous helper contract.
//! Compiler/version calls have no test-internal deadline. Failure artifacts are retained.

use std::{
    fs::{self, File},
    path::{Path, PathBuf},
    process::{Command, ExitStatus, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    thread,
    time::{Duration, Instant},
};

static NEXT: AtomicU64 = AtomicU64::new(0);

pub(super) struct Artifacts(PathBuf);

struct Run {
    status: ExitStatus,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

impl Artifacts {
    pub(super) fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "koven-ordered-map-{name}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed),
        ));
        fs::create_dir(&path).expect("unique ordered-map artifact directory");
        Self(path)
    }

    pub(super) fn write(&self, name: &str, contents: impl AsRef<[u8]>) -> PathBuf {
        let path = self.0.join(name);
        fs::write(&path, contents).expect("write ordered-map evidence");
        path
    }

    fn run(&self, label: &str, command: &mut Command, budget: Duration) -> Run {
        let command_text = format!("{command:?}\nbudget={budget:?}\n");
        self.write(&format!("{label}.command"), &command_text);
        let stdout_path = self.0.join(format!("{label}.stdout"));
        let stderr_path = self.0.join(format!("{label}.stderr"));
        // Files avoid pipe backpressure while the parent performs bounded try_wait.
        command
            .stdin(Stdio::null())
            .stdout(Stdio::from(File::create(&stdout_path).unwrap()))
            .stderr(Stdio::from(File::create(&stderr_path).unwrap()));
        let mut child = command.spawn().unwrap_or_else(|error| {
            panic!(
                "{label}: spawn failed: {error}\n{command_text}\nartifacts={}",
                self.0.display()
            )
        });
        let deadline = Instant::now() + budget;
        let mut failure = None;
        let mut status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break Some(status),
                Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(20)),
                Ok(None) => {
                    failure = Some(format!("timeout after {budget:?}"));
                    break None;
                }
                Err(error) => {
                    failure = Some(format!("try_wait failed: {error}"));
                    break None;
                }
            }
        };
        if let Some(reason) = &mut failure {
            // The generated fixture has no subprocess-spawning operation.
            // Kill only this Child, and never block indefinitely after kill fails.
            let child_kill = child.kill();
            let cleanup_deadline = Instant::now() + Duration::from_secs(2);
            let mut reap_error = None;
            loop {
                match child.try_wait() {
                    Ok(Some(reaped)) => {
                        status = Some(reaped);
                        break;
                    }
                    Ok(None) => {}
                    Err(error) => {
                        reap_error = Some(error.to_string());
                    }
                }
                if Instant::now() >= cleanup_deadline {
                    break;
                }
                thread::sleep(Duration::from_millis(20));
            }
            *reason = format!(
                "{reason}; child_kill={child_kill:?}; reaped={}; reap_error={reap_error:?}",
                status.is_some()
            );
            // Even failed kill/reap is a bounded infrastructure failure. There
            // is deliberately no unbounded Child::wait fallback on any path.
        }
        let stdout = fs::read(&stdout_path).unwrap();
        let stderr = fs::read(&stderr_path).unwrap();
        self.write(
            &format!("{label}.status"),
            format!("{status:?}\nfailure={failure:?}\n"),
        );
        assert!(
            failure.is_none() && status.is_some_and(|status| status.success()),
            "{label}: failure={failure:?}; status={status:?}\n{command_text}\nstdout:\n{}\nstderr:\n{}\nartifacts={}",
            String::from_utf8_lossy(&stdout),
            String::from_utf8_lossy(&stderr),
            self.0.display()
        );
        Run {
            status: status.expect("successful child was reaped"),
            stdout,
            stderr,
        }
    }

    fn compiler(&self, label: &str, command: &mut Command) -> Run {
        // Like the existing native LLVM helpers, compilation is synchronous;
        // its driver/cc1/linker tree is not claimed to have an internal deadline.
        let command_text = format!("{command:?}\ncompiler_deadline=none\n");
        self.write(&format!("{label}.command"), &command_text);
        let output = command.output().unwrap_or_else(|error| {
            panic!(
                "{label}: compiler launch failed: {error}\n{command_text}\nartifacts={}",
                self.0.display()
            )
        });
        self.write(&format!("{label}.stdout"), &output.stdout);
        self.write(&format!("{label}.stderr"), &output.stderr);
        self.write(&format!("{label}.status"), format!("{}\n", output.status));
        assert!(
            output.status.success(),
            "{label}: status={}\n{command_text}\nstdout:\n{}\nstderr:\n{}\nartifacts={}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
            self.0.display()
        );
        Run {
            status: output.status,
            stdout: output.stdout,
            stderr: output.stderr,
        }
    }

    pub(super) fn check_compiler(&self) {
        let run = self.compiler(
            "clang-version",
            Command::new(crate::test_support::ir_clang()).arg("--version"),
        );
        let version = String::from_utf8_lossy(&run.stdout);
        assert!(
            version.contains("clang version 21."),
            "LLVM 21 IR requires Clang 21: {version}; artifacts={}",
            self.0.display()
        );
    }

    pub(super) fn compile_and_run(
        &self,
        label: &str,
        ir: &Path,
        observer: Option<&Path>,
        stdout: &str,
        stderr: &str,
    ) {
        let executable = self.0.join(label);
        let mut command = Command::new(crate::test_support::ir_clang());
        command.arg("-O0").arg(ir);
        if let Some(observer) = observer {
            command.arg("-std=c11").arg(observer);
        }
        command.arg("-o").arg(&executable);
        self.compiler(&format!("{label}-compile"), &mut command);
        let run = self.run(
            &format!("{label}-run"),
            &mut Command::new(&executable),
            Duration::from_secs(10),
        );
        assert_eq!(run.status.code(), Some(0), "{label} must exit normally");
        assert!(
            run.stdout == stdout.as_bytes() && run.stderr == stderr.as_bytes(),
            "{label}: strict output mismatch\nexpected stdout:\n{stdout}\nactual stdout:\n{}\nexpected stderr:\n{stderr}\nactual stderr:\n{}\nartifacts={}",
            String::from_utf8_lossy(&run.stdout),
            String::from_utf8_lossy(&run.stderr),
            self.0.display()
        );
    }

    pub(super) fn finish(self) {
        // This is temporary evidence, not a claim of durable CI artifact publication.
        fs::remove_dir_all(&self.0).expect("remove successful owned fixture artifacts");
    }
}
