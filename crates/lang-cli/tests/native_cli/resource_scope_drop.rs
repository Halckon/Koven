//! Guide 10 双轨析构：容器/可空包裹中的资源类型按词法作用域结束析构，而非 ASAP。
use super::{OsStr, TestDirectory, fs, run};

/// 把值绑定到局部变量后不再使用，先打印标记行；deinit 输出必须在标记之后。
fn assert_scope_drop(name: &str, binding: &str) {
    let directory = TestDirectory::create();
    let source = directory.join(&format!("{name}.ko"));
    let program = format!(
        "class R(val name: String) {{ deinit() {{ println(\"deinit\") }} }}\nfun main(): Unit {{ {binding}; println(\"marker\") }}\n"
    );
    fs::write(&source, program).expect("write source");
    let executed = run([OsStr::new("run"), source.as_os_str()]);
    assert_eq!(executed.status.code(), Some(0), "{name}: {executed:?}");
    assert_eq!(executed.stdout, b"marker\ndeinit\n", "{name}: {executed:?}");
    assert!(executed.stderr.is_empty(), "{name}: {executed:?}");
}

#[test]
fn resource_in_list_drops_at_scope_end() {
    assert_scope_drop("list", "val items = listOf(R(\"x\"))");
}

#[test]
fn resource_in_nullable_drops_at_scope_end() {
    assert_scope_drop("nullable", "val item: R? = R(\"x\")");
}
