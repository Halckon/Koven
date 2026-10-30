//! Scoped cancellation for a single CLI run and its private child process group.

use std::{
    ffi::OsString,
    io::{self, Read},
    mem::MaybeUninit,
    os::unix::process::CommandExt,
    path::Path,
    process::{Child, Command, Output, Stdio},
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc::{self, Receiver, TryRecvError},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

const REPEATED: usize = 1 << 16;
const POLL: Duration = Duration::from_millis(10);
const GRACE: Duration = Duration::from_millis(500);
static SIGNAL: AtomicUsize = AtomicUsize::new(0);
static INSTALLED: AtomicBool = AtomicBool::new(false);

extern "C" fn record_signal(signal: libc::c_int) {
    // AtomicUsize is lock-free on both supported native hosts. The handler only
    // records intent; process-group operations belong to the controlling thread.
    let _ = SIGNAL.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |state| {
        Some(if state == 0 {
            signal as usize
        } else {
            state | REPEATED
        })
    });
}

pub(super) struct RunControl {
    interrupt: libc::sigaction,
    terminate: libc::sigaction,
    restored: bool,
}

pub(super) enum RunOutcome {
    CancelledBeforeStart(i32),
    Completed(Output),
}

impl RunControl {
    /// Keep this guard alive from before compilation until private cleanup ends.
    pub(super) fn install() -> io::Result<Self> {
        if INSTALLED
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            return Err(io::Error::other(
                "run cancellation handler already installed",
            ));
        }
        SIGNAL.store(0, Ordering::Relaxed);
        let result = Self::install_handlers();
        if result.is_err() {
            INSTALLED.store(false, Ordering::Release);
        }
        result
    }

    fn install_handlers() -> io::Result<Self> {
        // SAFETY: libc owns the host-specific sigaction layout; zero initialization
        // is valid and sigemptyset initializes the mask before it is passed to C.
        let mut action: libc::sigaction = unsafe { std::mem::zeroed() };
        action.sa_sigaction = record_signal as *const () as usize;
        action.sa_flags = libc::SA_RESTART;
        // SAFETY: all pointers refer to initialized, exclusively borrowed masks.
        unsafe {
            if libc::sigemptyset(&mut action.sa_mask) != 0
                || libc::sigaddset(&mut action.sa_mask, libc::SIGINT) != 0
                || libc::sigaddset(&mut action.sa_mask, libc::SIGTERM) != 0
            {
                return Err(io::Error::last_os_error());
            }
        }
        let interrupt = replace_handler(libc::SIGINT, &action)?;
        match replace_handler(libc::SIGTERM, &action) {
            Ok(terminate) => Ok(Self {
                interrupt,
                terminate,
                restored: false,
            }),
            Err(error) => {
                replace_handler(libc::SIGINT, &interrupt).map_err(|rollback| {
                    io::Error::other(format!(
                        "cannot install SIGTERM handler: {error}; cannot restore SIGINT: {rollback}"
                    ))
                })?;
                Err(error)
            }
        }
    }

    /// 显式恢复完整旧信号状态，错误交回 CLI；Drop 仅作兜底。
    pub(super) fn finish(mut self) -> io::Result<()> {
        self.restore()
    }

    fn restore(&mut self) -> io::Result<()> {
        if self.restored {
            return Ok(());
        }
        let terminate = replace_handler(libc::SIGTERM, &self.terminate).map(|_| ());
        let interrupt = replace_handler(libc::SIGINT, &self.interrupt).map(|_| ());
        match (terminate, interrupt) {
            (Ok(()), Ok(())) => {
                self.restored = true;
                INSTALLED.store(false, Ordering::Release);
                Ok(())
            }
            (Err(error), Ok(())) | (Ok(()), Err(error)) => Err(error),
            (Err(terminate), Err(interrupt)) => Err(io::Error::other(format!(
                "cannot restore SIGTERM: {terminate}; cannot restore SIGINT: {interrupt}"
            ))),
        }
    }

    pub(super) fn cancelled(&self) -> Option<i32> {
        let signal = SIGNAL.load(Ordering::Relaxed) & !REPEATED;
        (signal != 0).then_some(signal as i32)
    }

    pub(super) fn output(&self, executable: &Path, args: &[OsString]) -> io::Result<RunOutcome> {
        if let Some(signal) = self.cancelled() {
            return Ok(RunOutcome::CancelledBeforeStart(signal));
        }
        let mut child = Command::new(executable)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .process_group(0)
            .spawn()?;
        let readers = start_readers(&mut child);
        let (stdout, stderr) = match readers {
            Ok(readers) => readers,
            Err((error, reader)) => {
                let cleanup = terminate_and_reap(&mut child);
                if let Some(reader) = reader {
                    let _ = reader.thread.join();
                }
                cleanup?;
                return Err(error);
            }
        };
        let result = self.collect(&mut child, &stdout, &stderr);
        // On every control/read failure terminate the group before reaping the
        // leader, so pipe-owning descendants cannot leave the reader joins stuck.
        let cleanup = if result.is_err() {
            terminate_and_reap(&mut child)
        } else {
            Ok(())
        };
        let stdout_join = stdout.thread.join();
        let stderr_join = stderr.thread.join();
        cleanup?;
        if stdout_join.is_err() || stderr_join.is_err() {
            return Err(io::Error::other("run output reader panicked"));
        }
        result.map(RunOutcome::Completed)
    }

    fn collect(&self, child: &mut Child, stdout: &Reader, stderr: &Reader) -> io::Result<Output> {
        let mut out = None;
        let mut err = None;
        let mut forwarded = None;
        let mut killed = false;
        loop {
            stdout.receive(&mut out)?;
            stderr.receive(&mut err)?;
            if let Some(signal) = self.cancelled() {
                if forwarded.is_none() {
                    signal_group(child, signal)?;
                    forwarded = Some(Instant::now());
                }
                if !killed
                    && (SIGNAL.load(Ordering::Relaxed) & REPEATED != 0
                        || forwarded.is_some_and(|start| start.elapsed() >= GRACE))
                {
                    signal_group(child, libc::SIGKILL)?;
                    killed = true;
                }
            }
            // Do not reap while descendants may still hold pipes. The unreaped
            // leader reserves the process-group ID until the last possible kill.
            if out.is_some() && err.is_some() && (forwarded.is_none() || killed) {
                match child.try_wait() {
                    Ok(Some(status)) => {
                        return Ok(Output {
                            status,
                            stdout: out.unwrap_or_default(),
                            stderr: err.unwrap_or_default(),
                        });
                    }
                    Ok(None) => {}
                    Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                    Err(error) => return Err(error),
                }
            }
            thread::sleep(POLL);
        }
    }
}

impl Drop for RunControl {
    fn drop(&mut self) {
        let _ = self.restore();
    }
}

fn replace_handler(signal: libc::c_int, action: &libc::sigaction) -> io::Result<libc::sigaction> {
    let mut previous = MaybeUninit::uninit();
    // SAFETY: action has the libc host layout; previous is writable storage and
    // is read only after sigaction successfully initializes the entire action.
    if unsafe { libc::sigaction(signal, action, previous.as_mut_ptr()) } != 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: successful sigaction initialized previous above.
    Ok(unsafe { previous.assume_init() })
}

struct Reader {
    result: Receiver<io::Result<Vec<u8>>>,
    thread: JoinHandle<()>,
}

impl Reader {
    fn start(pipe: impl Read + Send + 'static) -> io::Result<Self> {
        let (sender, result) = mpsc::channel();
        let thread = thread::Builder::new().spawn(move || {
            let mut bytes = Vec::new();
            let mut pipe = pipe;
            let result = pipe.read_to_end(&mut bytes).map(|_| bytes);
            let _ = sender.send(result);
        })?;
        Ok(Self { result, thread })
    }

    fn receive(&self, value: &mut Option<Vec<u8>>) -> io::Result<()> {
        if value.is_none() {
            match self.result.try_recv() {
                Ok(result) => *value = Some(result?),
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => {
                    return Err(io::Error::other("run output reader disconnected"));
                }
            }
        }
        Ok(())
    }
}

type ReaderStartError = (io::Error, Option<Reader>);

fn start_readers(child: &mut Child) -> Result<(Reader, Reader), ReaderStartError> {
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| (io::Error::other("run stdout pipe missing"), None))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| (io::Error::other("run stderr pipe missing"), None))?;
    let stdout = Reader::start(stdout).map_err(|error| (error, None))?;
    match Reader::start(stderr) {
        Ok(stderr) => Ok((stdout, stderr)),
        Err(error) => Err((error, Some(stdout))),
    }
}

fn signal_group(child: &Child, signal: libc::c_int) -> io::Result<()> {
    // SAFETY: spawn created a dedicated group with this child's PID. Callers keep
    // the child unreaped until they have finished every group signal operation.
    if unsafe { libc::kill(-(child.id() as libc::pid_t), signal) } == 0 {
        return Ok(());
    }
    let error = io::Error::last_os_error();
    if error.raw_os_error() == Some(libc::ESRCH) {
        return Ok(());
    }
    #[cfg(target_os = "macos")]
    if error.raw_os_error() == Some(libc::EPERM) && exited_leader_only(child) {
        return Ok(());
    }
    Err(error)
}

/// Darwin may return EPERM for a group containing only its unreaped zombie leader.
/// Never generalize EPERM to success: positive, complete membership and exit proofs
/// are required, and waitid must not release the PID/PGID reservation.
#[cfg(target_os = "macos")]
fn exited_leader_only(child: &Child) -> bool {
    fn only_member(pid: libc::pid_t) -> bool {
        let mut members = [0 as libc::pid_t; 2];
        // SAFETY: this writable array has the supplied byte capacity. libproc's
        // convenience wrapper returns a PID count, not the number of bytes.
        let count = unsafe {
            libc::proc_listpgrppids(
                pid,
                members.as_mut_ptr().cast(),
                std::mem::size_of_val(&members) as libc::c_int,
            )
        };
        count == 1 && members[0] == pid
    }
    let pid = child.id() as libc::pid_t;
    if !only_member(pid) {
        return false;
    }
    // SAFETY: libc defines the host siginfo layout; waitid initializes this
    // writable structure. WNOWAIT keeps the child unreaped for subsequent control.
    let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
    let observed = unsafe {
        libc::waitid(
            libc::P_PID,
            child.id(),
            &mut info,
            libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
        )
    };
    observed == 0
        && info.si_pid == pid
        && info.si_signo == libc::SIGCHLD
        && matches!(
            info.si_code,
            libc::CLD_EXITED | libc::CLD_KILLED | libc::CLD_DUMPED
        )
        && only_member(pid)
}

fn terminate_and_reap(child: &mut Child) -> io::Result<()> {
    let signal = signal_group(child, libc::SIGKILL);
    if signal.is_err() {
        // A group-control error must still attempt to terminate the direct child.
        let _ = child.kill();
    }
    let reaped = loop {
        match child.wait() {
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            result => break result,
        }
    };
    signal?;
    reaped.map(|_| ())
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use std::{os::unix::process::ExitStatusExt, sync::Mutex};

    pub(crate) static SIGNAL_TEST: Mutex<()> = Mutex::new(());

    fn shell(control: &RunControl, source: &str) -> Output {
        match control
            .output(Path::new("/bin/sh"), &["-c".into(), source.into()])
            .unwrap()
        {
            RunOutcome::Completed(output) => output,
            RunOutcome::CancelledBeforeStart(_) => panic!("test child was not started"),
        }
    }

    #[test]
    fn finish_restores_original_handlers_masks_and_flags() {
        let _serial = SIGNAL_TEST.lock().unwrap();
        struct Original(i32, libc::sigaction);
        impl Drop for Original {
            fn drop(&mut self) {
                replace_handler(self.0, &self.1).unwrap();
            }
        }
        extern "C" fn original_handler(_: libc::c_int) {}
        fn current(signal: i32) -> libc::sigaction {
            let mut action = MaybeUninit::uninit();
            // SAFETY: a null new action only queries; success initializes action.
            assert_eq!(
                unsafe { libc::sigaction(signal, std::ptr::null(), action.as_mut_ptr()) },
                0
            );
            // SAFETY: successful query initialized action above.
            unsafe { action.assume_init() }
        }
        let mut originals = Vec::new();
        let mut expected = Vec::new();
        for (signal, blocked) in [
            (libc::SIGINT, libc::SIGUSR1),
            (libc::SIGTERM, libc::SIGUSR2),
        ] {
            // SAFETY: libc layout is zeroable; initialize mask before installation.
            let mut action: libc::sigaction = unsafe { std::mem::zeroed() };
            action.sa_sigaction = original_handler as *const () as usize;
            action.sa_flags = libc::SA_RESTART | libc::SA_NODEFER;
            // SAFETY: action owns initialized, exclusively borrowed mask storage.
            unsafe {
                assert_eq!(libc::sigemptyset(&mut action.sa_mask), 0);
                assert_eq!(libc::sigaddset(&mut action.sa_mask, blocked), 0);
            }
            originals.push(Original(signal, replace_handler(signal, &action).unwrap()));
            // Compare actual kernel state, including platform-added flag bits.
            expected.push((signal, current(signal)));
        }
        let control = RunControl::install().unwrap();
        control.finish().unwrap();
        for (signal, before) in expected {
            let after = current(signal);
            assert_eq!(after.sa_sigaction, before.sa_sigaction);
            assert_eq!(after.sa_flags, before.sa_flags);
            for member in 1..=31 {
                // SAFETY: both masks came from successful sigaction queries;
                // these signal numbers are valid on both supported hosts.
                unsafe {
                    assert_eq!(
                        libc::sigismember(&after.sa_mask, member),
                        libc::sigismember(&before.sa_mask, member)
                    );
                }
            }
        }
        drop(originals);
    }

    #[test]
    fn preserves_program_bytes_and_exit_status() {
        let _serial = SIGNAL_TEST.lock().unwrap();
        let control = RunControl::install().unwrap();
        let output = shell(&control, "printf out; printf err >&2; exit 7");
        assert_eq!(output.status.code(), Some(7));
        assert_eq!(output.stdout, b"out");
        assert_eq!(output.stderr, b"err");
        assert_eq!(control.cancelled(), None);
    }

    #[test]
    fn cancellation_before_spawn_never_launches_executable() {
        let _serial = SIGNAL_TEST.lock().unwrap();
        let control = RunControl::install().unwrap();
        record_signal(libc::SIGTERM);
        record_signal(libc::SIGINT);
        assert!(matches!(
            control.output(Path::new("/does-not-exist"), &[]).unwrap(),
            RunOutcome::CancelledBeforeStart(libc::SIGTERM)
        ));
    }

    #[test]
    fn ignoring_child_is_killed_after_grace_and_reaped() {
        let _serial = SIGNAL_TEST.lock().unwrap();
        let control = RunControl::install().unwrap();
        // Child readiness is established by its own signal after installing its
        // ignored disposition, rather than by a timing-dependent test sleep.
        let output = shell(
            &control,
            "trap '' TERM; printf ready; kill -TERM \"$PPID\"; while :; do :; done",
        );
        assert_eq!(control.cancelled(), Some(libc::SIGTERM));
        assert_eq!(output.status.signal(), Some(libc::SIGKILL));
        assert_eq!(output.stdout, b"ready");
    }

    #[test]
    fn default_child_termination_keeps_real_signal_status() {
        let _serial = SIGNAL_TEST.lock().unwrap();
        let control = RunControl::install().unwrap();
        let output = shell(&control, "kill -TERM \"$PPID\"; while :; do :; done");
        assert_eq!(control.cancelled(), Some(libc::SIGTERM));
        assert_eq!(output.status.signal(), Some(libc::SIGTERM));
    }

    #[test]
    fn exited_leader_does_not_hide_pipe_owning_descendant() {
        let _serial = SIGNAL_TEST.lock().unwrap();
        let control = RunControl::install().unwrap();
        // The leader can exit while its descendant retains both output pipes.
        // Delaying reap is necessary to safely cancel that group and drain EOF.
        let output = shell(
            &control,
            "trap '' TERM; sleep 30 & printf ready; kill -TERM \"$PPID\"; exit 0",
        );
        assert_eq!(control.cancelled(), Some(libc::SIGTERM));
        assert_eq!(output.status.code(), Some(0));
        assert_eq!(output.stdout, b"ready");
    }
}

#[cfg(test)]
mod closed_pipe_tests {
    use super::*;

    #[test]
    fn closed_pipes_do_not_allow_cancelled_descendant_to_survive() {
        let _serial = super::tests::SIGNAL_TEST.lock().unwrap();
        let control = RunControl::install().unwrap();
        let source = "trap '' TERM; (exec >/dev/null 2>&1; koven_control_closed_pipe_witness() { while :; do :; done; }; koven_control_closed_pipe_witness) & child=$!; printf '%s' \"$child\"; kill -TERM \"$PPID\"; exit 0";
        let output = match control
            .output(Path::new("/bin/sh"), &["-c".into(), source.into()])
            .unwrap()
        {
            RunOutcome::Completed(output) => output,
            RunOutcome::CancelledBeforeStart(_) => panic!("child must start"),
        };
        let pid: i32 = String::from_utf8(output.stdout).unwrap().parse().unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        let mut live;
        loop {
            let state = Command::new("ps")
                .args(["-p", &pid.to_string(), "-o", "stat="])
                .output()
                .unwrap();
            let status = String::from_utf8_lossy(&state.stdout);
            live = !status.trim().is_empty() && !status.trim().starts_with('Z');
            if !live || Instant::now() >= deadline {
                break;
            }
            thread::sleep(POLL);
        }
        if live {
            // SAFETY: the controlled fixture ignores TERM and stays alive until
            // this cleanup; its PID was returned by its still-running shell.
            unsafe {
                libc::kill(pid, libc::SIGKILL);
            }
        }
        control.finish().unwrap();
        assert!(
            !live,
            "cancelled pipe-closing descendant must be terminated"
        );
        assert_eq!(
            output.status.code(),
            Some(0),
            "leader's real status is preserved"
        );
    }
}
