//! Host-specific tools and object contracts shared by native regression tests.

use std::path::PathBuf;

pub(crate) fn clang() -> PathBuf {
    if cfg!(target_os = "macos") {
        PathBuf::from("/usr/bin/clang")
    } else {
        ir_clang()
    }
}

// Instrumented LLVM IR must be read by the same LLVM major that emitted it.
// The system macOS driver is still used above when linking existing objects.
pub(crate) fn ir_clang() -> PathBuf {
    std::env::var_os("LLVM_SYS_211_PREFIX")
        .map(PathBuf::from)
        .map(|prefix| prefix.join("bin/clang"))
        .filter(|path| path.is_file())
        .unwrap_or_else(|| PathBuf::from("clang"))
}

pub(crate) fn assert_native_object(bytes: &[u8]) {
    if cfg!(target_os = "macos") {
        assert_eq!(&bytes[..4], b"\xcf\xfa\xed\xfe");
        assert_eq!(&bytes[4..8], &[0x0c, 0x00, 0x00, 0x01]); // CPU_TYPE_ARM64
        assert_eq!(&bytes[12..16], &[0x01, 0x00, 0x00, 0x00]); // MH_OBJECT
    } else {
        assert_eq!(&bytes[..4], b"\x7fELF");
        assert_eq!(bytes[4], 2); // ELFCLASS64
        assert_eq!(bytes[5], 1); // ELFDATA2LSB
        assert_eq!(&bytes[16..18], &[1, 0]); // ET_REL
        assert_eq!(&bytes[18..20], &[62, 0]); // EM_X86_64
    }
}

pub(crate) fn main_symbol() -> &'static str {
    if cfg!(target_os = "macos") {
        "_main"
    } else {
        "main"
    }
}
