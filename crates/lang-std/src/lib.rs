//! Koven 标准库源码包的 Cargo 边界。

#[cfg(test)]
mod tests {
    use std::path::Path;

    #[test]
    fn koven_source_package_is_present() {
        let prelude = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("koven")
            .join("prelude.ko");

        assert!(prelude.is_file(), "lang-std 必须保留 Koven 源码真源");
        let ranges = Path::new(env!("CARGO_MANIFEST_DIR")).join("koven/algorithms/ranges.ko");
        assert!(ranges.is_file(), "通用 take 必须由标准库 Koven 源码实现");
    }
}
