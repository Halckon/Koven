//! 公开单文件 native build/run 命令编排。

use std::{
    ffi::OsString,
    fs, io,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use crate::{
    MessageFormat,
    bootstrap::{BootstrapEntry, BootstrapError, BootstrapTarget, bootstrap_build},
    format::CommandOutput,
    unit_build::UnitBuildError,
};

const BUILD_USAGE: &str =
    "usage: kovenc build <source.ko> [--source-root <dir>] [--entry <name>] -o <executable>\n";
const RUN_USAGE: &str =
    "usage: kovenc run <source.ko> [--source-root <dir>] [--entry <name>] [-- <program-arg>...]\n";
static NEXT_TEMPORARY: AtomicU64 = AtomicU64::new(0);

pub(super) fn execute_build(
    arguments: &[OsString],
    message_format: MessageFormat,
) -> CommandOutput {
    if arguments
        .first()
        .is_some_and(|argument| argument == "--project")
    {
        return crate::project_command::execute_build(arguments, message_format);
    }
    let (source, source_root, entry, executable) = match arguments {
        [
            source,
            root_flag,
            root,
            entry_flag,
            entry,
            output_flag,
            executable,
        ] if root_flag == "--source-root"
            && entry_flag == "--entry"
            && output_flag == "-o"
            && !is_option(source)
            && !is_option(root)
            && !is_option(entry)
            && !is_option(executable) =>
        {
            (
                PathBuf::from(source),
                Some(PathBuf::from(root)),
                Some(entry.to_string_lossy()),
                PathBuf::from(executable),
            )
        }
        [source, root_flag, root, output_flag, executable]
            if root_flag == "--source-root"
                && output_flag == "-o"
                && !is_option(source)
                && !is_option(root)
                && !is_option(executable) =>
        {
            (
                PathBuf::from(source),
                Some(PathBuf::from(root)),
                None,
                PathBuf::from(executable),
            )
        }

        [source, entry_flag, entry, output_flag, executable]
            if entry_flag == "--entry"
                && output_flag == "-o"
                && !is_option(source)
                && !is_option(entry)
                && !is_option(executable) =>
        {
            (
                PathBuf::from(source),
                None,
                Some(entry.to_string_lossy()),
                PathBuf::from(executable),
            )
        }
        [source, output_flag, executable]
            if output_flag == "-o" && !is_option(source) && !is_option(executable) =>
        {
            (PathBuf::from(source), None, None, PathBuf::from(executable))
        }
        [argument, ..] if is_unknown_option(argument, &["--source-root", "--entry", "-o"]) => {
            return usage(
                BUILD_USAGE,
                format!("unknown build option {}", argument.to_string_lossy()),
            );
        }
        _ => {
            return usage(
                BUILD_USAGE,
                if arguments.iter().any(|argument| argument == "--source-root") {
                    "expected source, optional `--source-root <dir>`, optional `--entry <name>` and `-o <executable>`"
                } else {
                    "expected source, optional `--entry <name>` and `-o <executable>`"
                },
            );
        }
    };
    let mut artifacts = match crate::build_artifacts::BuildArtifacts::reserve(&executable) {
        Ok(artifacts) => artifacts,
        Err(error) => {
            return CommandOutput::error(format!(
                "error: cannot reserve build artifacts: {error}\n"
            ));
        }
    };
    let result = bootstrap_build(
        BootstrapTarget {
            source: &source,
            source_root: source_root.as_deref(),
            entry: entry
                .as_deref()
                .map(BootstrapEntry::Explicit)
                .unwrap_or(BootstrapEntry::ConventionalMain),
            object: artifacts.object(),
            executable: artifacts.executable(),
        },
        None,
    );
    if let Err(error) = result {
        return render_error_with_cleanup(error, message_format, artifacts.cleanup().err());
    }
    match artifacts.publish(&executable) {
        Ok(()) => CommandOutput::success(Vec::new()),
        Err(error) => {
            let detail = match artifacts.cleanup() {
                Ok(()) => error.to_string(),
                Err(cleanup) => format!("{error}; build staging cleanup failed: {cleanup}"),
            };
            CommandOutput::error(format!(
                "error: build artifact finalization failed: {detail}\n"
            ))
        }
    }
}

pub(super) fn execute_run(arguments: &[OsString], message_format: MessageFormat) -> CommandOutput {
    if arguments
        .first()
        .is_some_and(|argument| argument == "--project")
    {
        return crate::project_command::execute_run(arguments, message_format);
    }
    let separator = arguments.iter().position(|argument| argument == "--");
    let (compiler_arguments, program_arguments) = match separator {
        Some(separator) => (&arguments[..separator], &arguments[separator + 1..]),
        None => (arguments, &[][..]),
    };
    let (source, source_root, entry) = match compiler_arguments {
        [source, root_flag, root, entry_flag, entry]
            if root_flag == "--source-root"
                && entry_flag == "--entry"
                && !is_option(source)
                && !is_option(root)
                && !is_option(entry) =>
        {
            (
                PathBuf::from(source),
                Some(PathBuf::from(root)),
                Some(entry.to_string_lossy()),
            )
        }
        [source, root_flag, root]
            if root_flag == "--source-root" && !is_option(source) && !is_option(root) =>
        {
            (PathBuf::from(source), Some(PathBuf::from(root)), None)
        }

        [source, entry_flag, entry]
            if entry_flag == "--entry" && !is_option(source) && !is_option(entry) =>
        {
            (PathBuf::from(source), None, Some(entry.to_string_lossy()))
        }
        [source] if !is_option(source) => (PathBuf::from(source), None, None),
        [argument, ..] if is_unknown_option(argument, &["--source-root", "--entry"]) => {
            return usage(
                RUN_USAGE,
                format!("unknown run option {}", argument.to_string_lossy()),
            );
        }
        _ => {
            return usage(
                RUN_USAGE,
                if compiler_arguments
                    .iter()
                    .any(|argument| argument == "--source-root")
                {
                    "expected source, optional `--source-root <dir>` and optional `--entry <name>`"
                } else {
                    "expected source and optional `--entry <name>`"
                },
            );
        }
    };
    let control = match crate::run_control::RunControl::install() {
        Ok(control) => control,
        Err(error) => {
            return CommandOutput::error(format!("error: cannot install run control: {error}\n"));
        }
    };
    let mut output = (|| {
        let directory = match TemporaryDirectory::create() {
            Ok(directory) => directory,
            Err(error) => {
                return CommandOutput::error(format!(
                    "error: cannot create run temporary directory: {error}\n"
                ));
            }
        };
        let object = directory.path().join("program.o");
        let executable = directory.path().join("program");
        if let Err(error) = bootstrap_build(
            BootstrapTarget {
                source: &source,
                source_root: source_root.as_deref(),
                entry: entry
                    .as_deref()
                    .map(BootstrapEntry::Explicit)
                    .unwrap_or(BootstrapEntry::ConventionalMain),
                object: &object,
                executable: &executable,
            },
            Some(&control),
        ) {
            let output = match error {
                BootstrapError::Linker(crate::linker::LinkerError::Cancelled { signal }) => {
                    CommandOutput {
                        status: (128 + signal) as u8,
                        stdout: Vec::new(),
                        stderr: Vec::new(),
                    }
                }
                error => render_error(error, message_format),
            };
            return finish_run_cleanup(output, directory.cleanup());
        }
        let output = match control.output(&executable, program_arguments) {
            Ok(crate::run_control::RunOutcome::Completed(output)) => CommandOutput {
                status: child_exit_status(output.status),
                stdout: output.stdout,
                stderr: output.stderr,
            },
            Ok(crate::run_control::RunOutcome::CancelledBeforeStart(signal)) => CommandOutput {
                status: (128 + signal) as u8,
                stdout: Vec::new(),
                stderr: Vec::new(),
            },
            Err(error) => CommandOutput::error(format!(
                "error: cannot launch {}: {error}\n",
                executable.display()
            )),
        };
        finish_run_cleanup(output, directory.cleanup())
    })();
    if let Err(error) = control.finish() {
        if output.status == 0 {
            output.status = 2;
        }
        if !output.stderr.is_empty() && !output.stderr.ends_with(b"\n") {
            output.stderr.push(b'\n');
        }
        output.stderr.extend_from_slice(
            format!("error: cannot restore run signal state: {error}\n").as_bytes(),
        );
    }
    output
}

/// 清理失败不能丢弃程序输出或覆盖已有非零退出状态。
fn finish_run_cleanup(mut output: CommandOutput, cleanup: io::Result<()>) -> CommandOutput {
    if let Err(error) = cleanup {
        if output.status == 0 {
            output.status = 2;
        }
        if !output.stderr.is_empty() && !output.stderr.ends_with(b"\n") {
            output.stderr.push(b'\n');
        }
        output.stderr.extend_from_slice(
            format!("error: cannot clean run temporary directory: {error}\n").as_bytes(),
        );
    }
    output
}

/// `kovenc run` exit status for the finished program (Guide v0.46 D9): its own exit code,
/// or `128 + signal` when a signal such as SIGABRT ended it, like a POSIX shell.
pub(super) fn child_exit_status(status: std::process::ExitStatus) -> u8 {
    if let Some(code) = status.code() {
        return u8::try_from(code).unwrap_or(1);
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(signal) = status.signal() {
            return u8::try_from(128 + signal).unwrap_or(u8::MAX);
        }
    }
    1
}

fn render_error(error: BootstrapError, message_format: MessageFormat) -> CommandOutput {
    match error {
        // 与 project 和 `check` 相同：frontend 诊断退出 1，usage/operational 保持 2。
        BootstrapError::Unit(UnitBuildError::FrontendDiagnostics { human, json }) => {
            let rendered = match message_format {
                MessageFormat::Human => human,
                MessageFormat::Json => json,
            };
            CommandOutput {
                status: 1,
                stdout: Vec::new(),
                stderr: rendered.into_bytes(),
            }
        }
        error => CommandOutput::error(format!("error: native build failed: {error}\n")),
    }
}

fn usage(usage: &str, message: impl AsRef<str>) -> CommandOutput {
    CommandOutput::error(format!("error: {}\n{usage}", message.as_ref()))
}

fn is_option(argument: &OsString) -> bool {
    argument.to_string_lossy().starts_with('-')
}

fn is_unknown_option(argument: &OsString, known: &[&str]) -> bool {
    is_option(argument) && !known.iter().any(|known| argument == known)
}

struct TemporaryDirectory(Option<PathBuf>);

impl TemporaryDirectory {
    fn create() -> io::Result<Self> {
        let path = std::env::temp_dir().join(format!(
            "koven-run-{}-{}",
            std::process::id(),
            NEXT_TEMPORARY.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path)?;
        Ok(Self(Some(path)))
    }

    fn path(&self) -> &Path {
        self.0.as_deref().expect("live temporary directory")
    }

    fn cleanup(mut self) -> io::Result<()> {
        let path = self.0.take().expect("live temporary directory");
        fs::remove_dir_all(path)
    }
}

impl Drop for TemporaryDirectory {
    fn drop(&mut self) {
        if let Some(path) = self.0.take() {
            let _ = fs::remove_dir_all(path);
        }
    }
}

/// 保留原始诊断协议，清理失败仅追加 operational error 并使退出状态为失败。
fn render_error_with_cleanup(
    error: BootstrapError,
    message_format: MessageFormat,
    cleanup: Option<io::Error>,
) -> CommandOutput {
    let mut output = render_error(error, message_format);
    if let Some(cleanup) = cleanup {
        output.status = 2;
        output.stderr.extend_from_slice(
            format!("error: build staging cleanup failed: {cleanup}\n").as_bytes(),
        );
    }
    output
}

#[cfg(test)]
mod cleanup_tests {
    use super::*;

    #[test]
    fn cleanup_failure_preserves_original_json_diagnostics() {
        let json = "{\"code\":\"L0001\"}\n";
        let output = render_error_with_cleanup(
            BootstrapError::Unit(UnitBuildError::FrontendDiagnostics {
                human: "human diagnostic\n".to_owned(),
                json: json.to_owned(),
            }),
            MessageFormat::Json,
            Some(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "cleanup denied",
            )),
        );
        assert_eq!(output.status, 2);
        assert!(output.stdout.is_empty());
        let stderr = String::from_utf8(output.stderr).expect("UTF-8 diagnostics");
        assert!(stderr.starts_with(json));
        assert!(!stderr.contains("human diagnostic"));
        assert!(stderr.contains("cleanup denied"));
    }
}

#[cfg(test)]
mod run_cleanup_tests {
    use super::*;

    #[test]
    fn cleanup_failure_retains_child_output_and_nonzero_exit() {
        let output = finish_run_cleanup(
            CommandOutput {
                status: 7,
                stdout: b"child stdout".to_vec(),
                stderr: b"child stderr".to_vec(),
            },
            Err(io::Error::other("cleanup fault")),
        );
        assert_eq!(output.status, 7);
        assert_eq!(output.stdout, b"child stdout");
        assert_eq!(
            output.stderr,
            b"child stderr\nerror: cannot clean run temporary directory: cleanup fault\n"
        );
    }

    #[test]
    fn cleanup_failure_turns_success_into_operational_failure() {
        let output = finish_run_cleanup(
            CommandOutput::success(b"result".to_vec()),
            Err(io::Error::other("cleanup fault")),
        );
        assert_eq!(output.status, 2);
        assert_eq!(output.stdout, b"result");
    }
}
