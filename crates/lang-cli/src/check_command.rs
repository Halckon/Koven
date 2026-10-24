//! `kovenc check`：只运行 build/run 共用的 frontend gate，不选择入口、不生成 object 或链接。
//!
//! 退出码：0 无诊断；1 frontend 诊断（与 build 相同的 human/JSON Lines 渲染）；
//! 2 usage 或 operational 错误。

use std::{ffi::OsString, path::Path};

use crate::{
    MessageFormat,
    bootstrap::{BootstrapError, prepare_single_source},
    format::CommandOutput,
    project_build::{ProjectBuildError, prepare_project_unit},
    unit_build::{UnitBuildError, check_unit_frontend},
};

const CHECK_USAGE: &str = "usage: kovenc check <source.ko> [--source-root <dir>]\n       kovenc check --project <project.toml>\n";

enum CheckFailure {
    Diagnostics { human: String, json: String },
    Operational(String),
}

pub(super) fn execute(arguments: &[OsString], message_format: MessageFormat) -> CommandOutput {
    let result = match arguments {
        [project_flag, manifest] if project_flag == "--project" && !is_option(manifest) => {
            check_project(Path::new(manifest))
        }
        [source] if !is_option(source) => check_single(Path::new(source), None),
        [source, root_flag, root]
            if root_flag == "--source-root" && !is_option(source) && !is_option(root) =>
        {
            check_single(Path::new(source), Some(Path::new(root)))
        }
        [argument, ..]
            if is_option(argument) && argument != "--project" && argument != "--source-root" =>
        {
            return usage(format!(
                "unknown check option {}",
                argument.to_string_lossy()
            ));
        }
        _ => {
            return usage(
                "expected source and optional `--source-root <dir>`, or `--project <project.toml>`",
            );
        }
    };
    match result {
        Ok(()) => CommandOutput::success(Vec::new()),
        Err(CheckFailure::Diagnostics { human, json }) => CommandOutput {
            status: 1,
            stdout: Vec::new(),
            stderr: match message_format {
                MessageFormat::Human => human.into_bytes(),
                MessageFormat::Json => json.into_bytes(),
            },
        },
        Err(CheckFailure::Operational(message)) => {
            CommandOutput::error(format!("error: check failed: {message}\n"))
        }
    }
}

fn check_single(source: &Path, source_root: Option<&Path>) -> Result<(), CheckFailure> {
    let prepared = prepare_single_source(source, source_root).map_err(single_failure)?;
    check_unit_frontend(&prepared.snapshot, &prepared.types)
        .map(|_| ())
        .map_err(unit_failure)
}

fn check_project(manifest: &Path) -> Result<(), CheckFailure> {
    let (snapshot, types) = prepare_project_unit(manifest, &[]).map_err(project_failure)?;
    check_unit_frontend(&snapshot, &types)
        .map(|_| ())
        .map_err(unit_failure)
}

fn single_failure(error: BootstrapError) -> CheckFailure {
    match error {
        BootstrapError::Unit(error) => unit_failure(error),
        error => CheckFailure::Operational(error.to_string()),
    }
}

fn project_failure(error: ProjectBuildError) -> CheckFailure {
    match error {
        ProjectBuildError::Unit(error) => unit_failure(error),
        error => CheckFailure::Operational(error.to_string()),
    }
}

fn unit_failure(error: UnitBuildError) -> CheckFailure {
    match error {
        UnitBuildError::FrontendDiagnostics { human, json } => {
            CheckFailure::Diagnostics { human, json }
        }
        error => CheckFailure::Operational(error.to_string()),
    }
}

fn usage(message: impl AsRef<str>) -> CommandOutput {
    CommandOutput::error(format!("error: {}\n{CHECK_USAGE}", message.as_ref()))
}

fn is_option(argument: &OsString) -> bool {
    argument.to_string_lossy().starts_with('-')
}
