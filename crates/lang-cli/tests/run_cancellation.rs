//! Catchable cancellation must reap the real run child before removing private files.
#![cfg(unix)]

use std::{
    fs,
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    thread,
    time::{Duration, Instant},
};

static NEXT: AtomicU64 = AtomicU64::new(0);

struct Fixture {
    root: PathBuf,
    cli: Option<Child>,
    program: Option<u32>,
}

impl Fixture {
    fn start(project: bool) -> Self {
        let root = std::env::temp_dir().join(format!(
            "koven-run-cancel-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(root.join("src")).unwrap();
        fs::create_dir(root.join("tmp")).unwrap();
        fs::write(
            root.join("src/main.ko"),
            "fun main(): Unit { while (true) {} }\n",
        )
        .unwrap();
        fs::write(root.join("project.toml"),
            "schema=\"koven.project\"\nversion=1\n[project]\nname=\"cancel\"\nsource-roots=[\"src\"]\n").unwrap();
        let mut command = Command::new(env!("CARGO_BIN_EXE_kovenc"));
        command.arg("run");
        if project {
            command.args(["--project", "project.toml", "--entry", "main"]);
        } else {
            command.arg("src/main.ko");
        }
        let child = command
            .current_dir(&root)
            .env("TMPDIR", root.join("tmp"))
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        Self {
            root,
            cli: Some(child),
            program: None,
        }
    }

    /// Wait for the actual native child, rather than guessing compiler duration.
    fn wait_for_program(&mut self) {
        let deadline = Instant::now() + Duration::from_secs(20);
        let cli = self.cli.as_mut().unwrap();
        loop {
            assert!(
                cli.try_wait().unwrap().is_none(),
                "CLI exited before program started"
            );
            let listing = Command::new("ps")
                .args(["-axo", "pid=,ppid=,comm="])
                .output()
                .unwrap();
            assert!(listing.status.success(), "ps failed: {listing:?}");
            for line in String::from_utf8_lossy(&listing.stdout).lines() {
                let mut fields = line.split_whitespace();
                let pid = fields.next().and_then(|value| value.parse::<u32>().ok());
                let parent = fields.next().and_then(|value| value.parse::<u32>().ok());
                let executable = fields.collect::<Vec<_>>().join(" ");
                if parent == Some(cli.id())
                    && (executable == "program" || executable.ends_with("/program"))
                {
                    self.program = pid;
                    return;
                }
            }
            assert!(Instant::now() < deadline, "native child did not start");
            thread::sleep(Duration::from_millis(10));
        }
    }

    fn cancel_and_verify(&mut self, signal: i32) {
        let cli = self.cli.as_mut().unwrap();
        assert!(
            Command::new("kill")
                .args([format!("-{signal}"), cli.id().to_string()])
                .status()
                .unwrap()
                .success()
        );
        let deadline = Instant::now() + Duration::from_secs(5);
        while cli.try_wait().unwrap().is_none() {
            assert!(Instant::now() < deadline, "cancelled CLI did not finish");
            thread::sleep(Duration::from_millis(10));
        }
        let output = self.cli.take().unwrap().wait_with_output().unwrap();
        assert_eq!(output.status.code(), Some(128 + signal), "{output:?}");
        assert!(
            output.stdout.is_empty() && output.stderr.is_empty(),
            "{output:?}"
        );
        let program = self.program.unwrap();
        let state = Command::new("ps")
            .args(["-p", &program.to_string(), "-o", "stat="])
            .output()
            .unwrap();
        assert!(state.stderr.is_empty(), "ps query failed: {state:?}");
        assert!(
            matches!(state.status.code(), Some(0 | 1)),
            "ps failed: {state:?}"
        );
        assert!(
            state.stdout.is_empty(),
            "native child must be reaped: {state:?}"
        );
        self.program = None;
        assert_eq!(
            fs::read_dir(self.root.join("tmp")).unwrap().count(),
            0,
            "private run artifacts must be removed before CLI exit"
        );
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        // Stop future spawns before inspecting leftover native processes.
        if let Some(cli) = &mut self.cli {
            let _ = cli.kill();
            let _ = cli.wait();
        }
        // A child forked immediately before CLI termination may still be finishing
        // exec. Bounded rescans also cover readiness failures without a recorded PID.
        let identity = self.root.file_name().unwrap().to_string_lossy();
        for _ in 0..3 {
            if let Ok(listing) = Command::new("ps")
                .args(["-ww", "-axo", "pid=,args="])
                .output()
            {
                for line in String::from_utf8_lossy(&listing.stdout).lines() {
                    if let Some((pid, command)) = line.trim().split_once(char::is_whitespace)
                        && pid.parse::<u32>().is_ok()
                        && command.contains(identity.as_ref())
                        && command.trim().ends_with("/program")
                    {
                        let _ = Command::new("kill").args(["-KILL", pid]).status();
                    }
                }
            }
            thread::sleep(Duration::from_millis(10));
        }
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[test]
fn single_run_cancellation_reaps_child_and_cleans_private_artifacts() {
    for signal in [libc::SIGINT, libc::SIGTERM] {
        let mut fixture = Fixture::start(false);
        fixture.wait_for_program();
        fixture.cancel_and_verify(signal);
    }
}

#[test]
fn project_run_cancellation_reaps_child_and_cleans_private_artifacts() {
    for signal in [libc::SIGINT, libc::SIGTERM] {
        let mut fixture = Fixture::start(true);
        fixture.wait_for_program();
        fixture.cancel_and_verify(signal);
    }
}
