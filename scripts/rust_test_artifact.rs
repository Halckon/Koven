//! Select the rlib actually linked by this Cargo integration target.
//! Cargo 1.96 fingerprints are a test implementation detail, not a public API.
//! Unknown or ambiguous identities fail closed; timestamps are never consulted.
use std::{
    fs,
    path::{Path, PathBuf},
};

pub(crate) fn for_current_test(deps: &Path, name: &str) -> PathBuf {
    let executable = std::env::current_exe().expect("locate current integration target");
    let stem = executable
        .file_stem()
        .and_then(|name| name.to_str())
        .expect("Cargo integration artifact has an ASCII name");
    let (target, hash) = stem
        .rsplit_once('-')
        .expect("Cargo integration artifact has a hash");
    let fingerprints = deps
        .parent()
        .expect("deps has profile directory")
        .join(".fingerprint");
    let target_fingerprint = fingerprints
        .join(format!("{}-{hash}", env!("CARGO_PKG_NAME")))
        .join(format!("test-integration-test-{target}.json"));
    let metadata =
        fs::read_to_string(&target_fingerprint).expect("read current target fingerprint");
    let marker = format!("\"{name}\",");
    let mut occurrences = metadata.match_indices(&marker);
    let (start, _) = occurrences
        .next()
        .expect("current target declares requested dependency");
    assert!(
        occurrences.next().is_none(),
        "dependency identity must be unique"
    );
    let (_, remainder) = metadata[start + marker.len()..]
        .split_once(',')
        .expect("Cargo dependency has a fingerprint after its public flag");
    let decimal = remainder
        .split([',', ']'])
        .next()
        .expect("dependency fingerprint")
        .trim();
    let expected: u64 = decimal
        .parse()
        .expect("Cargo dependency fingerprint is a u64");
    let prefix = format!("lib{name}-");
    let package = name.replace('_', "-");
    let mut candidates = fs::read_dir(deps)
        .expect("read current deps directory")
        .map(|entry| entry.expect("read dependency entry").path())
        .filter(|path| {
            let Some(hash) = path
                .file_name()
                .and_then(|name| name.to_str())
                .and_then(|file| file.strip_prefix(&prefix))
                .and_then(|file| file.strip_suffix(".rlib"))
            else {
                return false;
            };
            let fingerprint = fingerprints
                .join(format!("{package}-{hash}"))
                .join(format!("lib-{name}"));
            let Ok(text) = fs::read_to_string(fingerprint) else {
                return false;
            };
            let text = text.trim();
            if text.len() != 16 {
                return false;
            }
            let bytes = (0..8)
                .map(|index| u8::from_str_radix(&text[index * 2..index * 2 + 2], 16))
                .collect::<Result<Vec<_>, _>>()
                .expect("Cargo fingerprint is hexadecimal");
            u64::from_le_bytes(bytes.try_into().expect("eight fingerprint bytes")) == expected
        })
        .collect::<Vec<_>>();
    candidates.sort();
    assert_eq!(
        candidates.len(),
        1,
        "current target requires exactly one {name} rlib matching {expected}; candidates: {candidates:?}"
    );
    candidates.pop().expect("one matching artifact was checked")
}
