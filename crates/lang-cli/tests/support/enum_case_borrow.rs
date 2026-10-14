//! SPEC-0320: ordinary public processes, collected before the positive oracle.

use std::{fs, path::Path, process::Output};

pub const SOURCE: &str =
    include_str!("../../../lang-frontend/tests/fixtures/enum-case-borrow-0320/no-when.ko");

pub struct BuildRunCase {
    pub context: &'static str,
    pub built: Output,
    pub executed: Output,
    pub artifact: Option<Output>,
    pub executable_exists: bool,
    pub source_unchanged: bool,
    pub remaining_temporaries: Vec<String>,
}

pub fn remaining_temporaries(directory: &Path) -> Vec<String> {
    let mut paths = fs::read_dir(directory)
        .expect("read owned test directory")
        .map(|entry| entry.expect("read test directory entry"))
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            (name.contains(".kovenc-") || name.ends_with(".o")).then_some(name)
        })
        .collect::<Vec<_>>();
    paths.sort();
    paths
}

pub fn assert_successes(cases: &[BuildRunCase]) {
    for case in cases {
        for (command, output) in [("build", &case.built), ("run", &case.executed)] {
            eprintln!(
                "SPEC-0320 {} {command} status={:?}\nstdout: {:?}\nstderr: {:?}",
                case.context,
                output.status.code(),
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
        if let Some(output) = &case.artifact {
            eprintln!(
                "SPEC-0320 {} artifact status={:?}\nstdout: {:?}\nstderr: {:?}",
                case.context,
                output.status.code(),
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
    for case in cases {
        assert_eq!(
            case.built.status.code(),
            Some(0),
            "{}: {:?}",
            case.context,
            case.built
        );
        assert!(case.built.stdout.is_empty(), "{}", case.context);
        assert!(case.built.stderr.is_empty(), "{}", case.context);
        assert!(
            case.executable_exists,
            "{}: build must publish its executable",
            case.context
        );
        let artifact = case
            .artifact
            .as_ref()
            .expect("successful build artifact was run");
        for output in [artifact, &case.executed] {
            assert_eq!(
                output.status.code(),
                Some(0),
                "{}: {output:?}",
                case.context
            );
            assert_eq!(output.stdout, b"enum-borrow\n", "{}", case.context);
            assert!(output.stderr.is_empty(), "{}: {output:?}", case.context);
        }
        assert!(
            case.source_unchanged,
            "{}: source must stay unchanged",
            case.context
        );
        assert!(
            case.remaining_temporaries.is_empty(),
            "{}: {:?}",
            case.context,
            case.remaining_temporaries
        );
    }
}
