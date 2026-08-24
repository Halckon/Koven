//! `kovenc format` 的非破坏性文件与 frontend 编排。

use std::{ffi::OsString, fs, path::PathBuf};

use lang_frontend::{
    formatting::{FormattingError, format_source},
    source::SourceMap,
};

use crate::diagnostic_renderer::render_diagnostics;

const USAGE: &str = "usage: kovenc format [--check] <path>\n";

pub(super) struct CommandOutput {
    pub(super) status: u8,
    pub(super) stdout: Vec<u8>,
    pub(super) stderr: Vec<u8>,
}

impl CommandOutput {
    pub(super) fn usage(message: impl AsRef<str>) -> Self {
        Self::error(format!("error: {}\n{USAGE}", message.as_ref()))
    }

    pub(super) fn success(stdout: Vec<u8>) -> Self {
        Self {
            status: 0,
            stdout,
            stderr: Vec::new(),
        }
    }

    fn differs() -> Self {
        Self {
            status: 1,
            stdout: Vec::new(),
            stderr: Vec::new(),
        }
    }

    fn error(message: String) -> Self {
        Self {
            status: 2,
            stdout: Vec::new(),
            stderr: message.into_bytes(),
        }
    }
}

pub(super) fn execute(arguments: &[OsString]) -> CommandOutput {
    let (check, path) = match arguments {
        [path] if path != "--check" && !is_option(path) => (false, PathBuf::from(path)),
        [option, path] if option == "--check" && !is_option(path) => (true, PathBuf::from(path)),
        [option, ..] if is_option(option) && option != "--check" => {
            return CommandOutput::usage(format!(
                "unknown format option {}",
                option.to_string_lossy()
            ));
        }
        _ => return CommandOutput::usage("expected exactly one source path"),
    };

    let bytes = match fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) => {
            return CommandOutput::error(format!(
                "error: cannot read {}: {error}\n",
                path.display()
            ));
        }
    };
    let source = match String::from_utf8(bytes) {
        Ok(source) => source,
        Err(_) => {
            return CommandOutput::error(format!(
                "error: source {} is not valid UTF-8\n",
                path.display()
            ));
        }
    };

    let mut sources = SourceMap::new();
    let source_id = match sources.add_source(path.to_string_lossy(), source.clone()) {
        Ok(source_id) => source_id,
        Err(error) => {
            return CommandOutput::error(format!(
                "error: formatter source setup failed: {error}\n"
            ));
        }
    };
    let formatted = match format_source(&sources, source_id) {
        Ok(formatted) => formatted,
        Err(FormattingError::Diagnostics(diagnostics)) => {
            return match render_diagnostics(&sources, &diagnostics) {
                Ok(rendered) => CommandOutput::error(rendered),
                Err(error) => CommandOutput::error(format!(
                    "error: formatter diagnostic rendering failed: {error}\n"
                )),
            };
        }
        Err(error) => {
            return CommandOutput::error(format!("error: formatting failed: {error}\n"));
        }
    };

    if check {
        if formatted == source {
            CommandOutput::success(Vec::new())
        } else {
            CommandOutput::differs()
        }
    } else {
        CommandOutput::success(formatted.into_bytes())
    }
}

fn is_option(argument: &OsString) -> bool {
    argument.to_string_lossy().starts_with('-')
}
