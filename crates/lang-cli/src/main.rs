//! `kovenc` 编译器命令行入口。

mod bootstrap;
#[cfg(test)]
mod bootstrap_tests;

mod diagnostic_renderer;
mod format;
mod linker;
mod machine_diagnostic_renderer;
mod native_command;
mod project;
mod project_build;
mod project_command;
mod standard_sources;

use std::{ffi::OsString, io::Write, process::ExitCode};

use format::CommandOutput;

const GLOBAL_USAGE: &str = "usage: kovenc [--message-format=human|json] <format|build|run> ...\n";

#[derive(Clone, Copy)]
enum MessageFormat {
    Human,
    Json,
}

fn main() -> ExitCode {
    let arguments = std::env::args_os().skip(1).collect::<Vec<_>>();
    let mut stdout = std::io::stdout().lock();
    let mut stderr = std::io::stderr().lock();
    run(&arguments, &mut stdout, &mut stderr)
}

fn run(arguments: &[OsString], stdout: &mut dyn Write, stderr: &mut dyn Write) -> ExitCode {
    let (message_format, arguments) = match parse_global_options(arguments) {
        Ok(parsed) => parsed,
        Err(output) => return write_output(output, stdout, stderr),
    };
    let output = match arguments.split_first() {
        Some((command, rest)) if command == "format" => format::execute(rest, message_format),
        Some((command, rest)) if command == "build" => {
            native_command::execute_build(rest, message_format)
        }
        Some((command, rest)) if command == "run" => {
            native_command::execute_run(rest, message_format)
        }
        Some((command, _)) => global_usage(format!(
            "unknown command {}; expected `format`, `build` or `run`",
            command.to_string_lossy()
        )),
        None => global_usage("expected `format`, `build` or `run` command"),
    };

    write_output(output, stdout, stderr)
}

fn parse_global_options(
    arguments: &[OsString],
) -> Result<(MessageFormat, &[OsString]), CommandOutput> {
    let Some(first) = arguments.first() else {
        return Ok((MessageFormat::Human, arguments));
    };
    let first = first.to_string_lossy();
    let Some(value) = first.strip_prefix("--message-format=") else {
        if first == "--message-format" {
            return Err(global_usage("expected a value after `--message-format=`"));
        }
        return Ok((MessageFormat::Human, arguments));
    };
    let message_format = match value {
        "human" => MessageFormat::Human,
        "json" => MessageFormat::Json,
        _ => return Err(global_usage(format!("unknown message format {value}"))),
    };
    let remaining = &arguments[1..];
    if remaining
        .first()
        .is_some_and(|argument| argument.to_string_lossy().starts_with("--message-format"))
    {
        return Err(global_usage(
            "`--message-format` may only be specified once",
        ));
    }
    Ok((message_format, remaining))
}

fn global_usage(message: impl AsRef<str>) -> CommandOutput {
    CommandOutput::error(format!("error: {}\n{GLOBAL_USAGE}", message.as_ref()))
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
            "error: expected `format`, `build` or `run` command\n\
             usage: kovenc [--message-format=human|json] <format|build|run> ...\n"
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

    #[test]
    fn global_message_format_rejects_missing_unknown_and_duplicate_values() {
        for (arguments, expected) in [
            (
                vec![OsString::from("--message-format")],
                "expected a value after `--message-format=`",
            ),
            (
                vec![OsString::from("--message-format=yaml")],
                "unknown message format yaml",
            ),
            (
                vec![
                    OsString::from("--message-format=json"),
                    OsString::from("--message-format=human"),
                ],
                "may only be specified once",
            ),
        ] {
            let mut stdout = Vec::new();
            let mut stderr = Vec::new();
            assert_eq!(run(&arguments, &mut stdout, &mut stderr), ExitCode::from(2));
            assert!(stdout.is_empty());
            let stderr = String::from_utf8(stderr).expect("UTF-8 stderr");
            assert!(stderr.contains(expected), "{stderr}");
            assert!(stderr.contains("--message-format=human|json"), "{stderr}");
        }
    }
}
