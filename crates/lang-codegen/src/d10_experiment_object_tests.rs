//! Object inspection only: no linker, process-entry bridge, or execution.
use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};

struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        loop {
            let path = std::env::temp_dir().join(format!(
                "koven-d10-library-object-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed),
            ));
            match fs::create_dir(&path) {
                Ok(()) => return Self(path),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => panic!("create private object inspection directory: {error}"),
            }
        }
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

pub(crate) fn check(bytes: &[u8], callable_symbol: &str) {
    assert!(!bytes.is_empty());
    crate::test_support::assert_native_object(bytes);
    // Relocatable library object, not an executable/shared image.
    if cfg!(target_os = "macos") {
        assert_eq!(bytes.get(12..16), Some([1, 0, 0, 0].as_slice()));
    } else {
        assert_eq!(bytes.get(16..18), Some([1, 0].as_slice()));
    }
    let directory = Directory::new();
    let object = directory.0.join("library.o");
    fs::write(&object, bytes).expect("write inspection-only object");
    assert_eq!(fs::read(&object).unwrap(), bytes);
    // Include local symbols: the original Koven callable intentionally has internal linkage.
    let output = Command::new("/usr/bin/nm")
        .arg("-j")
        .arg(&object)
        .output()
        .expect("system nm must inspect library object");
    assert!(output.status.success(), "nm failed: {:?}", output.stderr);
    let symbols = String::from_utf8(output.stdout).expect("nm symbols must be UTF-8");
    let expected = if cfg!(target_os = "macos") {
        format!("_{callable_symbol}")
    } else {
        callable_symbol.to_owned()
    };
    assert_eq!(symbols.lines().filter(|line| *line == expected).count(), 1);
    let defined = Command::new("/usr/bin/nm")
        .args(if cfg!(target_os = "macos") {
            ["-j", "-U"]
        } else {
            ["-j", "--defined-only"]
        })
        .arg(&object)
        .output()
        .expect("system nm must inspect defined symbols");
    assert!(defined.status.success(), "nm failed: {:?}", defined.stderr);
    let defined = String::from_utf8(defined.stdout).expect("nm defined symbols must be UTF-8");
    assert_eq!(defined.lines().filter(|line| *line == expected).count(), 1);
    assert!(
        !symbols
            .lines()
            .any(|line| line == "main" || line == "_main")
    );
}
