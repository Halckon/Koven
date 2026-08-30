//! 公开无依赖本地 project build/run 命令与产物生命周期。

use std::{
    ffi::OsString,
    fs::{self, OpenOptions},
    io,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};

use lang_frontend::{
    lexer::{LexemeKind, TokenKind, lex},
    source::SourceMap,
};

use crate::{
    MessageFormat,
    format::CommandOutput,
    linker::link_native_object,
    project_build::{ProjectBuildError, emit_project_object},
};

const BUILD_USAGE: &str =
    "usage: kovenc build --project <project.toml> --entry <qualified-name> -o <executable>\n";
const RUN_USAGE: &str =
    "usage: kovenc run --project <project.toml> --entry <qualified-name> [-- <program-arg>...]\n";
static NEXT_TEMPORARY: AtomicU64 = AtomicU64::new(0);

pub(super) fn execute_build(
    arguments: &[OsString],
    message_format: MessageFormat,
) -> CommandOutput {
    let (manifest, selector, executable) = match arguments {
        [
            project_flag,
            manifest,
            entry_flag,
            selector,
            output_flag,
            executable,
        ] if project_flag == "--project"
            && entry_flag == "--entry"
            && output_flag == "-o"
            && !is_option(manifest)
            && !is_option(selector)
            && !is_option(executable) =>
        {
            let selector = match parse_selector(selector) {
                Ok(selector) => selector,
                Err(message) => return usage(BUILD_USAGE, message),
            };
            (PathBuf::from(manifest), selector, PathBuf::from(executable))
        }
        _ => {
            return usage(
                BUILD_USAGE,
                "expected fixed `--project`, `--entry` and `-o` arguments",
            );
        }
    };
    if manifest == executable {
        return operational("project manifest and final executable paths overlap");
    }
    if let Err(output) = require_absent_output(&executable) {
        return output;
    }
    let artifacts = match BuildArtifacts::reserve(&executable) {
        Ok(artifacts) => artifacts,
        Err(error) => return operational(format!("cannot reserve build temporaries: {error}")),
    };
    if let Err(error) = emit_project_object(
        &manifest,
        &selector,
        artifacts.object(),
        &[artifacts.object(), artifacts.executable(), &executable],
    ) {
        return render_project_error(error, message_format);
    }
    if let Err(error) = link_native_object(artifacts.object(), artifacts.executable()) {
        return operational(format!("linker failed: {error:?}"));
    }
    if let Err(error) = artifacts.commit(&executable) {
        return match error {
            ArtifactCommitError::CleanupObject(error) => operational(format!(
                "cannot clean temporary object before publishing {}: {error}",
                executable.display()
            )),
            ArtifactCommitError::Publish(error) => operational(format!(
                "cannot atomically publish {}: {error}",
                executable.display()
            )),
            ArtifactCommitError::CleanupPublished(error) => operational(format!(
                "published {} but cannot clean its temporary executable: {error}",
                executable.display()
            )),
        };
    }
    CommandOutput::success(Vec::new())
}

pub(super) fn execute_run(arguments: &[OsString], message_format: MessageFormat) -> CommandOutput {
    let separator = arguments.iter().position(|argument| argument == "--");
    let (compiler_arguments, program_arguments) = match separator {
        Some(separator) => (&arguments[..separator], &arguments[separator + 1..]),
        None => (arguments, &[][..]),
    };
    let (manifest, selector) = match compiler_arguments {
        [project_flag, manifest, entry_flag, selector]
            if project_flag == "--project"
                && entry_flag == "--entry"
                && !is_option(manifest)
                && !is_option(selector) =>
        {
            let selector = match parse_selector(selector) {
                Ok(selector) => selector,
                Err(message) => return usage(RUN_USAGE, message),
            };
            (PathBuf::from(manifest), selector)
        }
        _ => {
            return usage(
                RUN_USAGE,
                "expected fixed `--project` and `--entry` arguments",
            );
        }
    };
    let directory = match TemporaryDirectory::create() {
        Ok(directory) => directory,
        Err(error) => {
            return operational(format!("cannot create run temporary directory: {error}"));
        }
    };
    let object = directory.path().join("program.o");
    let executable = directory.path().join("program");
    if let Err(error) = emit_project_object(&manifest, &selector, &object, &[&object, &executable])
    {
        return render_project_error(error, message_format);
    }
    if let Err(error) = link_native_object(&object, &executable) {
        return operational(format!("linker failed: {error:?}"));
    }
    let mut output = match Command::new(&executable).args(program_arguments).output() {
        Ok(output) => CommandOutput {
            status: output
                .status
                .code()
                .and_then(|status| u8::try_from(status).ok())
                .unwrap_or(1),
            stdout: output.stdout,
            stderr: output.stderr,
        },
        Err(error) => operational(format!("cannot launch {}: {error}", executable.display())),
    };
    if let Err(error) = directory.cleanup() {
        append_cleanup_error(&mut output, &error);
    }
    output
}

fn append_cleanup_error(output: &mut CommandOutput, error: &io::Error) {
    if output.status == 0 {
        output.status = 1;
    }
    if !output.stderr.is_empty() && !output.stderr.ends_with(b"\n") {
        output.stderr.push(b'\n');
    }
    output.stderr.extend_from_slice(
        format!("error: cannot clean run temporary directory: {error}\n").as_bytes(),
    );
}

fn parse_selector(selector: &OsString) -> Result<Vec<String>, String> {
    let selector = selector
        .to_str()
        .ok_or_else(|| "project entry selector must be valid UTF-8".to_owned())?;
    let segments = selector.split('.').collect::<Vec<_>>();
    if segments.is_empty() || segments.iter().any(|segment| !is_koven_identifier(segment)) {
        return Err(format!(
            "project entry selector {selector:?} must be dot-separated Koven identifiers"
        ));
    }
    Ok(segments.into_iter().map(str::to_owned).collect())
}

fn is_option(argument: &OsString) -> bool {
    argument.to_string_lossy().starts_with('-')
}

fn is_koven_identifier(segment: &str) -> bool {
    let mut sources = SourceMap::new();
    let Ok(source) = sources.add_source("<project-entry-selector>", segment) else {
        return false;
    };
    let Ok(lexed) = lex(&sources, source) else {
        return false;
    };
    lexed.diagnostics().is_empty()
        && matches!(
            lexed.lexemes(),
            [identifier, eof]
                if identifier.kind() == LexemeKind::Token(TokenKind::Identifier)
                    && eof.kind() == LexemeKind::Eof
        )
}

fn require_absent_output(path: &Path) -> Result<(), CommandOutput> {
    match fs::symlink_metadata(path) {
        Ok(_) => Err(operational(format!(
            "output already exists: {}",
            path.display()
        ))),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(operational(format!(
            "cannot inspect output {}: {error}",
            path.display()
        ))),
    }
}

fn render_project_error(error: ProjectBuildError, message_format: MessageFormat) -> CommandOutput {
    match error {
        ProjectBuildError::FrontendDiagnostics { human, json } => CommandOutput {
            status: 1,
            stdout: Vec::new(),
            stderr: match message_format {
                MessageFormat::Human => human.into_bytes(),
                MessageFormat::Json => json.into_bytes(),
            },
        },
        error => operational(format!("project build failed: {error}")),
    }
}

fn usage(usage: &str, message: impl AsRef<str>) -> CommandOutput {
    CommandOutput::error(format!("error: {}\n{usage}", message.as_ref()))
}

fn operational(message: impl AsRef<str>) -> CommandOutput {
    CommandOutput {
        status: 1,
        stdout: Vec::new(),
        stderr: format!("error: {}\n", message.as_ref()).into_bytes(),
    }
}

struct BuildArtifacts {
    object: PathBuf,
    executable: PathBuf,
}

impl BuildArtifacts {
    fn reserve(final_output: &Path) -> io::Result<Self> {
        let parent = final_output.parent().unwrap_or_else(|| Path::new("."));
        let name = final_output
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("output");
        for _ in 0..128 {
            let nonce = NEXT_TEMPORARY.fetch_add(1, Ordering::Relaxed);
            let base = format!(".{name}.kovenc-{}-{nonce}", std::process::id());
            let object = parent.join(format!("{base}.o"));
            let executable = parent.join(format!("{base}.exe"));
            match reserve_file(&object) {
                Ok(()) => {}
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error),
            }
            match reserve_file(&executable) {
                Ok(()) => return Ok(Self { object, executable }),
                Err(error) => {
                    let _ = fs::remove_file(&object);
                    if error.kind() == io::ErrorKind::AlreadyExists {
                        continue;
                    }
                    return Err(error);
                }
            }
        }
        Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "could not reserve a unique sibling temporary",
        ))
    }

    fn object(&self) -> &Path {
        &self.object
    }

    fn executable(&self) -> &Path {
        &self.executable
    }

    fn commit(self, final_output: &Path) -> Result<(), ArtifactCommitError> {
        self.commit_with(final_output, |path| fs::remove_file(path))
    }

    fn commit_with(
        self,
        final_output: &Path,
        mut remove_file: impl FnMut(&Path) -> io::Result<()>,
    ) -> Result<(), ArtifactCommitError> {
        if let Err(error) = remove_file(&self.object)
            && error.kind() != io::ErrorKind::NotFound
        {
            return Err(ArtifactCommitError::CleanupObject(error));
        }
        fs::hard_link(&self.executable, final_output).map_err(ArtifactCommitError::Publish)?;
        if let Err(error) = remove_file(&self.executable)
            && error.kind() != io::ErrorKind::NotFound
        {
            return Err(ArtifactCommitError::CleanupPublished(error));
        }
        Ok(())
    }

    fn cleanup_files(&mut self) -> io::Result<()> {
        let mut first_error = None;
        for path in [&self.object, &self.executable] {
            if let Err(error) = fs::remove_file(path)
                && error.kind() != io::ErrorKind::NotFound
                && first_error.is_none()
            {
                first_error = Some(error);
            }
        }
        first_error.map_or(Ok(()), Err)
    }
}

#[derive(Debug)]
enum ArtifactCommitError {
    CleanupObject(io::Error),
    Publish(io::Error),
    CleanupPublished(io::Error),
}

impl Drop for BuildArtifacts {
    fn drop(&mut self) {
        let _ = self.cleanup_files();
    }
}

fn reserve_file(path: &Path) -> io::Result<()> {
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map(|_| ())
}

struct TemporaryDirectory(Option<PathBuf>);

impl TemporaryDirectory {
    fn create() -> io::Result<Self> {
        for _ in 0..128 {
            let path = std::env::temp_dir().join(format!(
                "koven-project-run-{}-{}",
                std::process::id(),
                NEXT_TEMPORARY.fetch_add(1, Ordering::Relaxed)
            ));
            match fs::create_dir(&path) {
                Ok(()) => return Ok(Self(Some(path))),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error),
            }
        }
        Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "could not reserve a unique run temporary directory",
        ))
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

#[cfg(test)]
mod tests {
    use std::{ffi::OsString, fs, io};

    use crate::{MessageFormat, format::CommandOutput};

    use super::{
        ArtifactCommitError, BUILD_USAGE, BuildArtifacts, RUN_USAGE, append_cleanup_error,
        execute_build, execute_run, link_native_object, parse_selector, require_absent_output,
        usage,
    };

    #[test]
    fn selector_accepts_only_dot_separated_koven_identifiers() {
        assert_eq!(
            parse_selector(&OsString::from("demo.tools.start")),
            Ok(vec![
                "demo".to_owned(),
                "tools".to_owned(),
                "start".to_owned()
            ])
        );
        assert_eq!(
            parse_selector(&OsString::from("start")),
            Ok(vec!["start".to_owned()])
        );
        for invalid in ["", ".start", "demo.", "demo..start", "demo.fun", "9start"] {
            assert!(
                parse_selector(&OsString::from(invalid)).is_err(),
                "{invalid}"
            );
        }
    }

    #[test]
    fn project_usage_has_fixed_public_forms() {
        let build = usage(BUILD_USAGE, "bad build");
        assert_eq!(build.status, 2);
        assert_eq!(
            String::from_utf8(build.stderr).expect("UTF-8 build usage"),
            "error: bad build\nusage: kovenc build --project <project.toml> --entry <qualified-name> -o <executable>\n"
        );
        let run = usage(RUN_USAGE, "bad run");
        assert_eq!(run.status, 2);
        assert_eq!(
            String::from_utf8(run.stderr).expect("UTF-8 run usage"),
            "error: bad run\nusage: kovenc run --project <project.toml> --entry <qualified-name> [-- <program-arg>...]\n"
        );
    }

    #[test]
    fn project_cli_rejects_missing_duplicate_unknown_and_reordered_options() {
        let malformed_builds = [
            vec![],
            vec!["--project"],
            vec!["--project", "project.toml"],
            vec!["--project", "project.toml", "--entry"],
            vec!["--project", "project.toml", "--entry", "app.start", "-o"],
            vec!["--project", "project.toml", "--entry", "app.start"],
            vec![
                "--project",
                "project.toml",
                "--entry",
                "app.start",
                "--entry",
                "again",
                "-o",
                "program",
            ],
            vec![
                "--project",
                "project.toml",
                "--unknown",
                "app.start",
                "-o",
                "program",
            ],
            vec![
                "--entry",
                "app.start",
                "--project",
                "project.toml",
                "-o",
                "program",
            ],
            vec![
                "--project",
                "--entry",
                "--entry",
                "app.start",
                "-o",
                "program",
            ],
            vec![
                "--project",
                "--project",
                "--entry",
                "app.start",
                "-o",
                "program",
            ],
            vec![
                "--project",
                "project.toml",
                "--entry",
                "app.start",
                "-o",
                "program",
                "-o",
                "again",
            ],
            vec![
                "--project",
                "project.toml",
                "--entry",
                "app.start",
                "-o",
                "-o",
            ],
        ];
        for arguments in malformed_builds {
            let arguments = arguments
                .into_iter()
                .map(OsString::from)
                .collect::<Vec<_>>();
            let output = execute_build(&arguments, MessageFormat::Human);
            assert_eq!(output.status, 2, "{arguments:?}");
            assert!(
                String::from_utf8(output.stderr)
                    .expect("UTF-8 usage")
                    .contains(BUILD_USAGE)
            );
        }

        let malformed_runs = [
            vec![],
            vec!["--project"],
            vec!["--project", "project.toml", "--entry"],
            vec!["--project", "project.toml"],
            vec![
                "--project",
                "project.toml",
                "--entry",
                "app.start",
                "--entry",
                "again",
            ],
            vec!["--project", "project.toml", "--unknown", "app.start"],
            vec!["--project", "--entry", "--entry", "app.start"],
            vec!["--project", "--project", "--entry", "app.start"],
            vec!["--project", "project.toml", "--entry", "app.start", "extra"],
        ];
        for arguments in malformed_runs {
            let arguments = arguments
                .into_iter()
                .map(OsString::from)
                .collect::<Vec<_>>();
            let output = execute_run(&arguments, MessageFormat::Human);
            assert_eq!(output.status, 2, "{arguments:?}");
            assert!(
                String::from_utf8(output.stderr)
                    .expect("UTF-8 usage")
                    .contains(RUN_USAGE)
            );
        }
    }

    #[test]
    fn atomic_commit_never_replaces_a_racing_final_output() {
        let directory = std::env::temp_dir().join(format!(
            "koven-project-artifact-test-{}-{}",
            std::process::id(),
            super::NEXT_TEMPORARY.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        fs::create_dir(&directory).expect("create artifact test directory");
        let final_output = directory.join("program");
        let artifacts = BuildArtifacts::reserve(&final_output).expect("reserve artifacts");
        fs::write(artifacts.executable(), b"complete executable").expect("seed linked output");
        fs::write(&final_output, b"racing caller output").expect("seed race winner");
        let error = artifacts
            .commit(&final_output)
            .expect_err("no-replace commit must reject the race winner");
        assert!(matches!(
            error,
            ArtifactCommitError::Publish(error)
                if error.kind() == io::ErrorKind::AlreadyExists
        ));
        assert_eq!(
            fs::read(&final_output).expect("read final"),
            b"racing caller output"
        );
        assert_eq!(fs::read_dir(&directory).expect("read directory").count(), 1);
        fs::remove_dir_all(directory).expect("clean artifact test directory");
    }

    #[test]
    fn atomic_commit_does_not_publish_when_object_cleanup_fails() {
        let directory = std::env::temp_dir().join(format!(
            "koven-project-artifact-cleanup-test-{}-{}",
            std::process::id(),
            super::NEXT_TEMPORARY.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        fs::create_dir(&directory).expect("create artifact test directory");
        let final_output = directory.join("program");
        let artifacts = BuildArtifacts::reserve(&final_output).expect("reserve artifacts");
        let object = artifacts.object().to_path_buf();
        fs::write(artifacts.executable(), b"complete executable").expect("seed linked output");
        fs::remove_file(&object).expect("remove reserved object");
        fs::create_dir(&object).expect("inject object cleanup failure");

        let error = artifacts
            .commit(&final_output)
            .expect_err("cleanup failure must prevent publication");
        assert!(matches!(error, ArtifactCommitError::CleanupObject(_)));
        assert!(!final_output.exists());

        fs::remove_dir(&object).expect("remove injected directory");
        fs::remove_dir_all(directory).expect("clean artifact test directory");
    }

    #[test]
    fn atomic_commit_reports_cleanup_failure_after_publishing_complete_output() {
        let directory = std::env::temp_dir().join(format!(
            "koven-project-artifact-published-cleanup-test-{}-{}",
            std::process::id(),
            super::NEXT_TEMPORARY.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        fs::create_dir(&directory).expect("create artifact test directory");
        let final_output = directory.join("program");
        let artifacts = BuildArtifacts::reserve(&final_output).expect("reserve artifacts");
        let temporary_executable = artifacts.executable().to_path_buf();
        fs::write(&temporary_executable, b"complete executable").expect("seed linked output");

        let error = artifacts
            .commit_with(&final_output, |path| {
                if path == temporary_executable {
                    Err(io::Error::other("injected published cleanup failure"))
                } else {
                    fs::remove_file(path)
                }
            })
            .expect_err("published cleanup failure must be observable");
        assert!(matches!(error, ArtifactCommitError::CleanupPublished(_)));
        assert_eq!(
            fs::read(&final_output).expect("read published output"),
            b"complete executable"
        );

        fs::remove_dir_all(directory).expect("clean artifact test directory");
    }

    #[test]
    fn linker_failure_does_not_publish_and_artifact_drop_cleans_siblings() {
        let directory = std::env::temp_dir().join(format!(
            "koven-project-link-failure-test-{}-{}",
            std::process::id(),
            super::NEXT_TEMPORARY.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        fs::create_dir(&directory).expect("create artifact test directory");
        let final_output = directory.join("program");
        let artifacts = BuildArtifacts::reserve(&final_output).expect("reserve artifacts");
        fs::write(artifacts.object(), b"not an object").expect("seed invalid object");

        assert!(link_native_object(artifacts.object(), artifacts.executable()).is_err());
        drop(artifacts);
        assert!(!final_output.exists());
        assert_eq!(fs::read_dir(&directory).expect("read directory").count(), 0);

        fs::remove_dir_all(directory).expect("clean artifact test directory");
    }

    #[test]
    fn run_directory_cleanup_failure_is_observable() {
        let directory = super::TemporaryDirectory::create().expect("reserve run directory");
        let path = directory.path().to_path_buf();
        fs::remove_dir(&path).expect("remove reserved directory");
        fs::write(&path, b"not a directory").expect("inject cleanup failure");

        let error = directory
            .cleanup()
            .expect_err("remove_dir_all on a file must fail");
        assert_eq!(error.kind(), io::ErrorKind::NotADirectory);

        fs::remove_file(path).expect("clean injected file");
    }

    #[test]
    fn cleanup_failure_preserves_program_output_and_nonzero_status() {
        let error = io::Error::other("injected cleanup failure");
        let mut success = CommandOutput {
            status: 0,
            stdout: b"stdout\n".to_vec(),
            stderr: b"stderr".to_vec(),
        };
        append_cleanup_error(&mut success, &error);
        assert_eq!(success.status, 1);
        assert_eq!(success.stdout, b"stdout\n");
        assert!(String::from_utf8_lossy(&success.stderr).starts_with("stderr\nerror:"));

        let mut failure = CommandOutput {
            status: 7,
            stdout: b"partial\n".to_vec(),
            stderr: Vec::new(),
        };
        append_cleanup_error(&mut failure, &error);
        assert_eq!(failure.status, 7);
        assert_eq!(failure.stdout, b"partial\n");
        assert!(String::from_utf8_lossy(&failure.stderr).contains("injected cleanup failure"));
    }

    #[cfg(unix)]
    #[test]
    fn output_preflight_rejects_dangling_symlinks() {
        use std::os::unix::fs::symlink;

        let directory = std::env::temp_dir().join(format!(
            "koven-project-symlink-test-{}-{}",
            std::process::id(),
            super::NEXT_TEMPORARY.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        fs::create_dir(&directory).expect("create symlink test directory");
        let output = directory.join("program");
        symlink(directory.join("missing"), &output).expect("create dangling symlink");
        let rejected = require_absent_output(&output).expect_err("symlink counts as existing");
        assert_eq!(rejected.status, 1);
        fs::remove_dir_all(directory).expect("clean symlink test directory");
    }
}
