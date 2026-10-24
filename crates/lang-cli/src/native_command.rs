//! 公开单文件 native build/run 命令编排。

use std::{
    ffi::OsString,
    fs, io,
    path::{Path, PathBuf},
    process::Command,
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
    let object = temporary_object_path(&executable);
    let result = bootstrap_build(BootstrapTarget {
        source: &source,
        source_root: source_root.as_deref(),
        entry: entry
            .as_deref()
            .map(BootstrapEntry::Explicit)
            .unwrap_or(BootstrapEntry::ConventionalMain),
        object: &object,
        executable: &executable,
    });
    if let Err(error) = fs::remove_file(&object)
        && error.kind() != io::ErrorKind::NotFound
    {
        return CommandOutput::error(format!(
            "error: cannot clean temporary object {}: {error}\n",
            object.display()
        ));
    }
    match result {
        Ok(()) => CommandOutput::success(Vec::new()),
        Err(error) => render_error(error, message_format),
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
    if let Err(error) = bootstrap_build(BootstrapTarget {
        source: &source,
        source_root: source_root.as_deref(),
        entry: entry
            .as_deref()
            .map(BootstrapEntry::Explicit)
            .unwrap_or(BootstrapEntry::ConventionalMain),
        object: &object,
        executable: &executable,
    }) {
        return render_error(error, message_format);
    }
    let output = match Command::new(&executable).args(program_arguments).output() {
        Ok(output) => CommandOutput {
            status: child_exit_status(output.status),
            stdout: output.stdout,
            stderr: output.stderr,
        },
        Err(error) => CommandOutput::error(format!(
            "error: cannot launch {}: {error}\n",
            executable.display()
        )),
    };
    match directory.cleanup() {
        Ok(()) => output,
        Err(error) => CommandOutput::error(format!(
            "error: cannot clean run temporary directory: {error}\n"
        )),
    }
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

fn temporary_object_path(executable: &Path) -> PathBuf {
    let parent = executable.parent().unwrap_or_else(|| Path::new("."));
    let name = executable
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("output");
    parent.join(format!(
        ".{name}.kovenc-{}-{}.o",
        std::process::id(),
        NEXT_TEMPORARY.fetch_add(1, Ordering::Relaxed)
    ))
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
