//! `kovenc` 编译器命令行入口。

#[allow(
    dead_code,
    reason = "SPEC-0042 bootstrap driver precedes the public CLI argument contract"
)]
mod bootstrap;
#[cfg(test)]
mod bootstrap_tests;

// SPEC-0057 复用纯 renderer 输出 formatter 诊断；公开 build 流水线仍未接入。
mod diagnostic_renderer;
mod format;
#[allow(
    dead_code,
    reason = "SPEC-0039 linker precedes source entry and CLI pipeline wiring"
)]
mod linker;

use std::{ffi::OsString, io::Write, process::ExitCode};

use format::CommandOutput;

fn main() -> ExitCode {
    let arguments = std::env::args_os().skip(1).collect::<Vec<_>>();
    let mut stdout = std::io::stdout().lock();
    let mut stderr = std::io::stderr().lock();
    run(&arguments, &mut stdout, &mut stderr)
}

fn run(arguments: &[OsString], stdout: &mut dyn Write, stderr: &mut dyn Write) -> ExitCode {
    let output = match arguments.split_first() {
        Some((command, rest)) if command == "format" => format::execute(rest),
        Some((command, _)) => CommandOutput::usage(format!(
            "unknown command {}; expected `format`",
            command.to_string_lossy()
        )),
        None => CommandOutput::usage("expected `format` command"),
    };

    write_output(output, stdout, stderr)
}

fn write_output(output: CommandOutput, stdout: &mut dyn Write, stderr: &mut dyn Write) -> ExitCode {
    if stdout.write_all(&output.stdout).is_err()
        || stderr.write_all(&output.stderr).is_err()
        || stdout.flush().is_err()
        || stderr.flush().is_err()
    {
        return ExitCode::from(2);
    }
    ExitCode::from(output.status)
}

#[cfg(test)]
mod tests {
    use std::{ffi::OsString, io, process::ExitCode};

    use crate::format::CommandOutput;

    use super::{run, write_output};

    struct FailingWriter;

    impl io::Write for FailingWriter {
        fn write(&mut self, _buffer: &[u8]) -> io::Result<usize> {
            Err(io::Error::other("intentional write failure"))
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn missing_unknown_and_output_failures_exit_two_without_panicking() {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        assert_eq!(run(&[], &mut stdout, &mut stderr), ExitCode::from(2));
        assert!(stdout.is_empty());
        assert_eq!(
            String::from_utf8(stderr).expect("UTF-8 stderr"),
            "error: expected `format` command\nusage: kovenc format [--check] <path>\n"
        );

        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        assert_eq!(
            run(&[OsString::from("unknown")], &mut stdout, &mut stderr),
            ExitCode::from(2)
        );
        assert!(
            String::from_utf8(stderr)
                .expect("UTF-8 stderr")
                .contains("unknown command unknown")
        );

        assert_eq!(
            write_output(
                CommandOutput::success(b"formatted".to_vec()),
                &mut FailingWriter,
                &mut Vec::new()
            ),
            ExitCode::from(2)
        );
        assert_eq!(
            write_output(
                CommandOutput::usage("failure"),
                &mut Vec::new(),
                &mut FailingWriter
            ),
            ExitCode::from(2)
        );
    }
}
