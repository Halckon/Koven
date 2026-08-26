use std::{
    fs, io,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use super::{
    ProjectLoadError,
    error::{IoOperation, SourceRootError, Utf8Subject},
    load_project_source_set,
    manifest::parse_manifest,
    model::ProjectSourceSet,
};

static NEXT_TEMPORARY: AtomicU64 = AtomicU64::new(0);

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn new(label: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "koven-project-{label}-{}-{}",
            std::process::id(),
            NEXT_TEMPORARY.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).expect("create project test directory");
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn manifest(roots: &[&str]) -> String {
    let roots = roots
        .iter()
        .map(|root| format!("{root:?}"))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "schema = \"koven.project\"\nversion = 1\n\n[project]\nname = \"hello-world\"\nsource-roots = [{roots}]\n"
    )
}

fn write_file(path: &Path, bytes: impl AsRef<[u8]>) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create file parent");
    }
    fs::write(path, bytes).expect("write project fixture");
}

fn write_project_manifest(directory: &Path, roots: &[&str]) -> PathBuf {
    let path = directory.join("project.toml");
    write_file(&path, manifest(roots));
    path
}

#[test]
fn strict_manifest_accepts_only_version_one_schema() {
    let path = Path::new("project.toml");
    let parsed =
        parse_manifest(path, &manifest(&["src", "generated"])).expect("valid version 1 manifest");
    assert_eq!(parsed.name, "hello-world");
    assert_eq!(parsed.source_roots, ["generated", "src"]);

    for (source, assertion) in [
        (
            "version = 1\n[project]\nname = \"hello\"\nsource-roots = [\"src\"]\n",
            0,
        ),
        (
            "schema = \"other\"\nversion = 1\n[project]\nname = \"hello\"\nsource-roots = [\"src\"]\n",
            1,
        ),
        (
            "schema = \"koven.project\"\nversion = 2\n[project]\nname = \"hello\"\nsource-roots = [\"src\"]\n",
            2,
        ),
        (
            "schema = \"koven.project\"\nversion = \"1\"\n[project]\nname = \"hello\"\nsource-roots = [\"src\"]\n",
            3,
        ),
    ] {
        let error = parse_manifest(path, source).expect_err("invalid schema/version");
        assert!(
            matches!(
                (assertion, error),
                (0, ProjectLoadError::MissingField { field: "schema" })
                    | (1, ProjectLoadError::UnsupportedSchema { .. })
                    | (2, ProjectLoadError::UnsupportedVersion { found: 2 })
                    | (
                        3,
                        ProjectLoadError::InvalidFieldType {
                            field: "version",
                            ..
                        }
                    )
            ),
            "unexpected schema/version error"
        );
    }
}

#[test]
fn strict_manifest_rejects_unknown_fields_and_invalid_values() {
    let path = Path::new("project.toml");
    for field in ["dependencies", "target", "entry"] {
        let source = manifest(&["src"]).replacen(
            "version = 1\n",
            &format!("version = 1\n{field} = true\n"),
            1,
        );
        assert!(matches!(
            parse_manifest(path, &source),
            Err(ProjectLoadError::UnknownField {
                section: "manifest",
                field: found,
            }) if found == field
        ));
    }
    let source = format!("{}unknown = true\n", manifest(&["src"]));
    assert!(matches!(
        parse_manifest(path, &source),
        Err(ProjectLoadError::UnknownField {
            section: "project",
            field,
        }) if field == "unknown"
    ));

    for name in ["", "1hello", "héllo", "hello.world"] {
        let source = manifest(&["src"]).replace("hello-world", name);
        assert!(matches!(
            parse_manifest(path, &source),
            Err(ProjectLoadError::InvalidProjectName { .. })
        ));
    }
    assert!(matches!(
        parse_manifest(path, &manifest(&[])),
        Err(ProjectLoadError::EmptySourceRoots)
    ));
    assert!(matches!(
        parse_manifest(path, &manifest(&["src", "src"])),
        Err(ProjectLoadError::DuplicateSourceRoot { root }) if root == "src"
    ));
    let non_string = manifest(&["src"]).replace("[\"src\"]", "[1]");
    assert!(matches!(
        parse_manifest(path, &non_string),
        Err(ProjectLoadError::InvalidFieldType {
            field: "project.source-roots[]",
            ..
        })
    ));
}

#[test]
fn strict_manifest_rejects_missing_and_wrongly_typed_fields() {
    let path = Path::new("project.toml");
    for (source, field) in [
        ("schema = \"koven.project\"\nversion = 1\n", "project"),
        (
            "schema = \"koven.project\"\nversion = 1\n[project]\nsource-roots = [\"src\"]\n",
            "project.name",
        ),
        (
            "schema = \"koven.project\"\nversion = 1\n[project]\nname = \"hello\"\n",
            "project.source-roots",
        ),
    ] {
        assert!(matches!(
            parse_manifest(path, source),
            Err(ProjectLoadError::MissingField { field: found }) if found == field
        ));
    }

    for (source, field) in [
        (
            "schema = 1\nversion = 1\n[project]\nname = \"hello\"\nsource-roots = [\"src\"]\n",
            "schema",
        ),
        (
            "schema = \"koven.project\"\nversion = 1\nproject = \"hello\"\n",
            "project",
        ),
        (
            "schema = \"koven.project\"\nversion = 1\n[project]\nname = 1\nsource-roots = [\"src\"]\n",
            "project.name",
        ),
        (
            "schema = \"koven.project\"\nversion = 1\n[project]\nname = \"hello\"\nsource-roots = \"src\"\n",
            "project.source-roots",
        ),
    ] {
        assert!(matches!(
            parse_manifest(path, source),
            Err(ProjectLoadError::InvalidFieldType { field: found, .. }) if found == field
        ));
    }

    let first = manifest(&["src"]).replacen(
        "version = 1\n",
        "version = 1\nzeta = true\nalpha = true\n",
        1,
    );
    let second = manifest(&["src"]).replacen(
        "version = 1\n",
        "version = 1\nalpha = true\nzeta = true\n",
        1,
    );
    for source in [first, second] {
        assert!(matches!(
            parse_manifest(path, &source),
            Err(ProjectLoadError::UnknownField { field, .. }) if field == "alpha"
        ));
    }
}

#[test]
fn source_root_validation_is_portable_and_precise() {
    let path = Path::new("project.toml");
    for (root, reason) in [
        ("", SourceRootError::Empty),
        ("/src", SourceRootError::Absolute),
        ("C:/src", SourceRootError::Absolute),
        ("src//main", SourceRootError::EmptySegment),
        ("./src", SourceRootError::CurrentSegment),
        ("src/../main", SourceRootError::ParentSegment),
        ("src\\main", SourceRootError::Backslash),
        ("src/", SourceRootError::TrailingSeparator),
        ("safe/C:/src", SourceRootError::WindowsPrefix),
        ("safe/C:relative", SourceRootError::WindowsPrefix),
    ] {
        assert!(matches!(
            parse_manifest(path, &manifest(&[root])),
            Err(ProjectLoadError::InvalidSourceRoot {
                reason: found,
                ..
            }) if found == reason
        ));
    }
}

#[test]
fn invalid_root_error_selection_is_independent_of_manifest_order() {
    let path = Path::new("project.toml");
    let first = parse_manifest(path, &manifest(&["/bad", ""])).expect_err("invalid roots");
    let second = parse_manifest(path, &manifest(&["", "/bad"])).expect_err("invalid roots");
    assert_eq!(first.to_string(), second.to_string());
    assert!(matches!(
        first,
        ProjectLoadError::InvalidSourceRoot {
            root,
            reason: SourceRootError::Empty,
        } if root.is_empty()
    ));

    let first =
        parse_manifest(path, &manifest(&["z", "a", "z", "a"])).expect_err("duplicate roots");
    let second =
        parse_manifest(path, &manifest(&["a", "z", "a", "z"])).expect_err("duplicate roots");
    assert_eq!(first.to_string(), second.to_string());
    assert!(matches!(
        first,
        ProjectLoadError::DuplicateSourceRoot { root } if root == "a"
    ));
}

#[test]
fn discovery_loads_sorted_utf8_sources_and_ignores_non_sources() {
    let directory = TestDirectory::new("positive");
    let manifest_path = write_project_manifest(directory.path(), &["src", "generated", "empty"]);
    fs::create_dir(directory.path().join("empty")).expect("create empty root");
    write_file(
        &directory.path().join("src/app/Zed.ko"),
        "package app\nfun zed(): Unit {}\n",
    );
    write_file(
        &directory.path().join("src/app/Main.ko"),
        "package app\nfun main(): Unit {}\n",
    );
    write_file(
        &directory.path().join("generated/app/Generated.ko"),
        "package app\nfun generated(): Unit {}\n",
    );
    write_file(
        &directory.path().join("src/.hidden.ko"),
        "fun hidden(): Unit {}\n",
    );
    write_file(&directory.path().join("src/ignored.txt"), b"\xff");
    write_file(&directory.path().join("src/ignored.KO"), b"\xff");
    write_file(&directory.path().join("src/ignored.ko~"), b"\xff");

    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(
            directory.path().join("src/app/Main.ko"),
            directory.path().join("src/linked.ko"),
        )
        .expect("create ignored source symlink");
        std::os::unix::fs::symlink(
            directory.path().join("src/app"),
            directory.path().join("src/linked-directory"),
        )
        .expect("create ignored directory symlink");
    }

    let snapshot = load_project_source_set(&manifest_path).expect("load project snapshot");
    assert_eq!(snapshot.name(), "hello-world");
    assert_eq!(
        snapshot
            .roots()
            .iter()
            .map(|root| root.identity())
            .collect::<Vec<_>>(),
        ["empty", "generated", "src"]
    );
    assert_eq!(
        snapshot
            .sources()
            .iter()
            .map(|source| (source.root_identity(), source.logical_path()))
            .collect::<Vec<_>>(),
        [
            ("generated", "app/Generated.ko"),
            ("src", ".hidden.ko"),
            ("src", "app/Main.ko"),
            ("src", "app/Zed.ko"),
        ]
    );
    assert!(snapshot.sources().iter().all(|source| {
        source.presentation_path().starts_with(directory.path()) && !source.text().is_empty()
    }));
    assert!(
        snapshot
            .roots()
            .iter()
            .all(|root| { root.presentation_path().starts_with(directory.path()) })
    );
    assert_eq!(
        snapshot.roots()[2].presentation_path(),
        directory.path().join("src")
    );
    assert_eq!(
        snapshot.sources()[2].presentation_path(),
        directory.path().join("src/app/Main.ko")
    );
}

#[test]
fn a_project_with_only_an_empty_root_produces_an_empty_snapshot() {
    let directory = TestDirectory::new("empty-snapshot");
    let manifest_path = write_project_manifest(directory.path(), &["src"]);
    fs::create_dir(directory.path().join("src")).expect("create empty source root");

    let snapshot = load_project_source_set(&manifest_path).expect("load empty project snapshot");
    assert_eq!(snapshot.roots().len(), 1);
    assert!(snapshot.sources().is_empty());
}

#[test]
fn root_and_file_order_and_project_location_do_not_change_identity() {
    fn create_project(directory: &Path, roots: &[&str], reverse_files: bool) -> PathBuf {
        let manifest_path = write_project_manifest(directory, roots);
        let files = [
            ("src/app/Main.ko", "package app\nfun main(): Unit {}\n"),
            (
                "generated/app/Helper.ko",
                "package app\nfun helper(): Unit {}\n",
            ),
        ];
        let order: &[usize] = if reverse_files { &[1, 0] } else { &[0, 1] };
        for index in order {
            write_file(&directory.join(files[*index].0), files[*index].1);
        }
        manifest_path
    }

    let first = TestDirectory::new("relocate-a");
    let second = TestDirectory::new("relocate-b");
    let first_manifest = create_project(first.path(), &["src", "generated"], false);
    let second_manifest = create_project(second.path(), &["generated", "src"], true);
    let first_snapshot = load_project_source_set(&first_manifest).expect("first snapshot");
    let second_snapshot = load_project_source_set(&second_manifest).expect("second snapshot");

    let portable = |snapshot: &ProjectSourceSet| {
        snapshot
            .sources()
            .iter()
            .map(|source| {
                (
                    source.root_identity().to_owned(),
                    source.logical_path().to_owned(),
                    source.text().to_owned(),
                )
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(portable(&first_snapshot), portable(&second_snapshot));
    assert_ne!(
        first_snapshot.sources()[0].presentation_path(),
        second_snapshot.sources()[0].presentation_path()
    );
}

#[test]
fn manifest_io_and_utf8_fail_before_discovery_without_language_codes() {
    let directory = TestDirectory::new("manifest-errors");
    let wrong_name = directory.path().join("other.toml");
    write_file(&wrong_name, manifest(&["src"]));
    let error = load_project_source_set(&wrong_name).expect_err("wrong manifest file name");
    assert!(matches!(
        error,
        ProjectLoadError::InvalidManifestFileName { .. }
    ));

    let missing = directory.path().join("project.toml");
    let error = load_project_source_set(&missing).expect_err("missing manifest");
    assert!(matches!(
        error,
        ProjectLoadError::Io {
            operation: IoOperation::ReadManifest,
            ..
        }
    ));
    assert!(!error.to_string().contains("L000"));

    write_file(&missing, b"\xff");
    assert!(matches!(
        load_project_source_set(&missing),
        Err(ProjectLoadError::InvalidUtf8 {
            subject: Utf8Subject::Manifest,
            ..
        })
    ));
    write_file(&missing, "not = [valid");
    assert!(matches!(
        load_project_source_set(&missing),
        Err(ProjectLoadError::InvalidToml { .. })
    ));
}

#[test]
fn roots_must_exist_be_directories_and_not_overlap() {
    let missing_project = TestDirectory::new("missing-root");
    let manifest_path = write_project_manifest(missing_project.path(), &["missing"]);
    assert!(matches!(
        load_project_source_set(&manifest_path),
        Err(ProjectLoadError::MissingSourceRoot { root, .. }) if root == "missing"
    ));

    let file_project = TestDirectory::new("file-root");
    let manifest_path = write_project_manifest(file_project.path(), &["src"]);
    write_file(&file_project.path().join("src"), "not a directory");
    assert!(matches!(
        load_project_source_set(&manifest_path),
        Err(ProjectLoadError::SourceRootNotDirectory { root, .. }) if root == "src"
    ));

    let overlap_project = TestDirectory::new("overlap-root");
    let manifest_path =
        write_project_manifest(overlap_project.path(), &["src/nested", "src-aux", "src"]);
    fs::create_dir_all(overlap_project.path().join("src/nested")).expect("create nested roots");
    fs::create_dir(overlap_project.path().join("src-aux")).expect("create disjoint root");
    assert!(matches!(
        load_project_source_set(&manifest_path),
        Err(ProjectLoadError::OverlappingSourceRoots { first, second })
            if first == "src" && second == "src/nested"
    ));
}

#[cfg(unix)]
#[test]
fn root_symlinks_are_rejected_but_nested_symlinks_are_ignored() {
    use std::os::unix::fs::symlink;

    let directory = TestDirectory::new("root-symlink");
    let manifest_path = write_project_manifest(directory.path(), &["linked/src"]);
    let outside = TestDirectory::new("root-symlink-target");
    fs::create_dir(outside.path().join("src")).expect("create target root");
    symlink(outside.path(), directory.path().join("linked")).expect("create intermediate symlink");
    assert!(matches!(
        load_project_source_set(&manifest_path),
        Err(ProjectLoadError::SymlinkSourceRoot { root, .. }) if root == "linked/src"
    ));

    let nested = TestDirectory::new("nested-symlink");
    let manifest_path = write_project_manifest(nested.path(), &["src"]);
    fs::create_dir(nested.path().join("src")).expect("create source root");
    write_file(
        &outside.path().join("outside.ko"),
        "fun outside(): Unit {}\n",
    );
    symlink(
        outside.path().join("outside.ko"),
        nested.path().join("src/linked.ko"),
    )
    .expect("create nested source symlink");
    assert!(
        load_project_source_set(&manifest_path)
            .expect("ignore nested symlink")
            .sources()
            .is_empty()
    );
}

#[test]
fn invalid_source_utf8_and_hard_link_duplicates_are_operational_errors() {
    let utf8_project = TestDirectory::new("source-utf8");
    let manifest_path = write_project_manifest(utf8_project.path(), &["src"]);
    write_file(&utf8_project.path().join("src/Bad.ko"), b"\xff");
    assert!(matches!(
        load_project_source_set(&manifest_path),
        Err(ProjectLoadError::InvalidUtf8 {
            subject: Utf8Subject::Source,
            ..
        })
    ));

    let duplicate_project = TestDirectory::new("hard-link");
    let manifest_path = write_project_manifest(duplicate_project.path(), &["a", "b"]);
    let first = duplicate_project.path().join("a/Shared.ko");
    let second = duplicate_project.path().join("b/Shared.ko");
    write_file(&first, "fun shared(): Unit {}\n");
    fs::create_dir_all(second.parent().expect("second parent")).expect("create second root");
    fs::hard_link(&first, &second).expect("create duplicate physical source");
    assert!(matches!(
        load_project_source_set(&manifest_path),
        Err(ProjectLoadError::DuplicatePhysicalSource { .. })
    ));
}

#[cfg(unix)]
#[test]
fn unreadable_directory_and_source_report_io_operations() {
    use std::os::unix::fs::PermissionsExt;

    let directory_project = TestDirectory::new("directory-permission");
    let manifest_path = write_project_manifest(directory_project.path(), &["src"]);
    let source_root = directory_project.path().join("src");
    fs::create_dir(&source_root).expect("create unreadable source root");
    let original = fs::metadata(&source_root)
        .expect("source root metadata")
        .permissions();
    fs::set_permissions(&source_root, fs::Permissions::from_mode(0o111))
        .expect("remove directory read permission");
    let directory_result = load_project_source_set(&manifest_path);
    fs::set_permissions(&source_root, original).expect("restore directory permission");
    assert!(matches!(
        directory_result,
        Err(ProjectLoadError::Io {
            operation: IoOperation::ReadDirectory,
            ..
        })
    ));

    let source_project = TestDirectory::new("source-permission");
    let manifest_path = write_project_manifest(source_project.path(), &["src"]);
    let source_path = source_project.path().join("src/Main.ko");
    write_file(&source_path, "fun main(): Unit {}\n");
    let original = fs::metadata(&source_path)
        .expect("source metadata")
        .permissions();
    fs::set_permissions(&source_path, fs::Permissions::from_mode(0o000))
        .expect("remove source read permission");
    let source_result = load_project_source_set(&manifest_path);
    fs::set_permissions(&source_path, original).expect("restore source permission");
    assert!(matches!(
        source_result,
        Err(ProjectLoadError::Io {
            operation: IoOperation::ReadSource,
            ..
        })
    ));
}

#[cfg(unix)]
#[test]
fn non_utf8_included_paths_are_rejected() {
    use std::{ffi::OsString, os::unix::ffi::OsStringExt};

    let invalid = OsString::from_vec(b"bad\xff.ko".to_vec());
    assert!(matches!(
        super::discovery::utf8_path_segment(invalid, PathBuf::from("src/<invalid>.ko")),
        Err(ProjectLoadError::InvalidSourcePathEncoding { .. })
    ));
}

#[cfg(all(unix, not(target_os = "macos")))]
#[test]
fn real_non_utf8_source_entries_are_rejected_during_discovery() {
    use std::{ffi::OsString, os::unix::ffi::OsStringExt};

    let file_project = TestDirectory::new("non-utf8-file");
    let manifest_path = write_project_manifest(file_project.path(), &["src"]);
    let mut invalid_file = file_project.path().join("src");
    invalid_file.push(OsString::from_vec(b"bad\xff.ko".to_vec()));
    write_file(&invalid_file, "fun invalid(): Unit {}\n");
    assert!(matches!(
        load_project_source_set(&manifest_path),
        Err(ProjectLoadError::InvalidSourcePathEncoding { .. })
    ));

    let directory_project = TestDirectory::new("non-utf8-directory");
    let manifest_path = write_project_manifest(directory_project.path(), &["src"]);
    let mut invalid_directory = directory_project.path().join("src");
    invalid_directory.push(OsString::from_vec(b"bad\xff".to_vec()));
    write_file(
        &invalid_directory.join("Main.ko"),
        "fun invalid(): Unit {}\n",
    );
    assert!(matches!(
        load_project_source_set(&manifest_path),
        Err(ProjectLoadError::InvalidSourcePathEncoding { .. })
    ));
}

#[test]
fn directory_iteration_error_selection_is_stable() {
    let first = super::discovery::select_directory_error(vec![
        io::Error::from_raw_os_error(13),
        io::Error::from_raw_os_error(2),
    ])
    .expect("select first error");
    let second = super::discovery::select_directory_error(vec![
        io::Error::from_raw_os_error(2),
        io::Error::from_raw_os_error(13),
    ])
    .expect("select second error");
    assert_eq!(first.kind(), second.kind());
    assert_eq!(first.raw_os_error(), second.raw_os_error());
}

#[test]
fn root_error_selection_is_independent_of_manifest_order() {
    let first = TestDirectory::new("root-order-a");
    let second = TestDirectory::new("root-order-b");
    let first_manifest = write_project_manifest(first.path(), &["z", "a"]);
    let second_manifest = write_project_manifest(second.path(), &["a", "z"]);
    for manifest_path in [first_manifest, second_manifest] {
        assert!(matches!(
            load_project_source_set(&manifest_path),
            Err(ProjectLoadError::MissingSourceRoot { root, .. }) if root == "a"
        ));
    }
}
