//! 默认 build 的最终产物合同；调试包不是泄漏的临时目录。
use std::path::Path;

#[cfg(target_os = "macos")]
use std::path::PathBuf;

pub(super) fn assert_debug_bundle(executable: &Path) {
    #[cfg(target_os = "macos")]
    {
        let mut name = executable.as_os_str().to_os_string();
        name.push(".dSYM");
        let bundle = PathBuf::from(name);
        assert!(
            bundle.join("Contents/Info.plist").is_file(),
            "missing debug package metadata"
        );
        assert!(
            bundle
                .join("Contents/Resources/DWARF")
                .join(executable.file_name().expect("executable name"))
                .is_file(),
            "missing matching executable debug data"
        );
    }
    #[cfg(not(target_os = "macos"))]
    let _ = executable;
}

pub(super) fn is_final_output(executable: &Path, relative: &Path) -> bool {
    let name = executable.file_name().expect("output name");
    if relative == Path::new(name) {
        return true;
    }
    #[cfg(target_os = "macos")]
    {
        let mut bundle = name.to_os_string();
        bundle.push(".dSYM");
        relative.starts_with(PathBuf::from(bundle))
    }
    #[cfg(not(target_os = "macos"))]
    false
}
