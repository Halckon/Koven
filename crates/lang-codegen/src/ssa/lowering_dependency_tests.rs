//! SPEC-0255 的有界词法回归门禁，不是 rustc 的完整依赖图。
//!
//! 只扫描已知职责文件及物理子树；忽略注释、字面量和精确的外层 #[cfg(test)]
//! item，不解释 cfg_attr、宏展开、include!、#[path] 或扫描范围外的 re-export。
//! 禁用名字是保守的 identifier 规则（同名局部变量也会拒绝），无需解析 Rust 类型。
//! IR 的 SSA 根 glob/module alias 另行禁止，避免藏起 support-owned 名字；IR 内部
//! 的 glob 仍允许。模块深度包含普通 inline mod，不做名称解析或 alias 数据流分析。

use std::{fs, path::Path};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Boundary {
    Unit,
    Support,
    Ir,
}

#[derive(Clone, Copy, Debug)]
struct Token<'a> {
    text: &'a str,
    offset: usize,
}

fn boundary(path: &Path) -> Option<Boundary> {
    let name = path.file_name()?.to_str()?;
    if !name.ends_with(".rs") || name.ends_with("_tests.rs") {
        return None;
    }
    let first = path.components().next()?.as_os_str().to_str()?;
    let root = first.strip_suffix(".rs").unwrap_or(first);
    match root {
        "unit_lower" | "unit_plan" => Some(Boundary::Unit),
        "lowering_support" => Some(Boundary::Support),
        "model" | "types" => Some(Boundary::Ir),
        _ if root.starts_with("verify") && !root.ends_with("_tests") => Some(Boundary::Ir),
        _ => None,
    }
}

fn identifier_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

fn quoted_end(bytes: &[u8], start: usize, quote: u8) -> usize {
    let mut index = start + 1;
    while index < bytes.len() {
        match bytes[index] {
            b'\\' => index += 2,
            byte if byte == quote => return index + 1,
            _ => index += 1,
        }
    }
    bytes.len()
}

fn raw_string_end(bytes: &[u8], start: usize) -> Option<usize> {
    let mut index = start;
    if matches!(bytes.get(index), Some(b'b' | b'c')) {
        index += 1;
    }
    if bytes.get(index) != Some(&b'r') {
        return None;
    }
    index += 1;
    let hashes_start = index;
    while bytes.get(index) == Some(&b'#') {
        index += 1;
    }
    let hashes = index - hashes_start;
    if bytes.get(index) != Some(&b'"') {
        return None;
    }
    index += 1;
    while index < bytes.len() {
        if bytes[index] == b'"'
            && bytes.get(index + 1..index + 1 + hashes)
                == Some(&bytes[hashes_start..hashes_start + hashes])
        {
            return Some(index + 1 + hashes);
        }
        index += 1;
    }
    Some(bytes.len())
}

fn tokens(source: &str) -> Vec<Token<'_>> {
    let bytes = source.as_bytes();
    let mut result = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index].is_ascii_whitespace() {
            index += 1;
        } else if bytes[index..].starts_with(b"//") {
            while index < bytes.len() && bytes[index] != b'\n' {
                index += 1;
            }
        } else if bytes[index..].starts_with(b"/*") {
            index += 2;
            let mut depth = 1;
            while index < bytes.len() && depth != 0 {
                if bytes[index..].starts_with(b"/*") {
                    depth += 1;
                    index += 2;
                } else if bytes[index..].starts_with(b"*/") {
                    depth -= 1;
                    index += 2;
                } else {
                    index += 1;
                }
            }
        } else if let Some(end) = raw_string_end(bytes, index) {
            index = end;
        } else if bytes[index] == b'"' {
            index = quoted_end(bytes, index, b'"');
        } else if matches!(bytes[index], b'b' | b'c') && bytes.get(index + 1) == Some(&b'"') {
            index = quoted_end(bytes, index + 1, b'"');
        } else if bytes[index] == b'b' && bytes.get(index + 1) == Some(&b'\'') {
            index = quoted_end(bytes, index + 1, b'\'');
        } else if bytes[index] == b'\''
            && (bytes.get(index + 1) == Some(&b'\\')
                || source[index + 1..]
                    .chars()
                    .next()
                    .is_some_and(|ch| bytes.get(index + 1 + ch.len_utf8()) == Some(&b'\'')))
        {
            index = quoted_end(bytes, index, b'\'');
        } else {
            let offset = index;
            // r#ident 是 identifier，不应与 raw string 一起被忽略。
            if bytes[index..].starts_with(b"r#") {
                index += 2;
            }
            let start = index;
            if identifier_byte(bytes[index]) {
                while index < bytes.len() && identifier_byte(bytes[index]) {
                    index += 1;
                }
            } else if bytes[index..].starts_with(b"::") {
                index += 2;
            } else {
                index += source[index..]
                    .chars()
                    .next()
                    .expect("character")
                    .len_utf8();
            }
            result.push(Token {
                text: &source[start..index],
                offset,
            });
        }
    }
    result
}

fn at(tokens: &[Token<'_>], index: usize, text: &str) -> bool {
    tokens.get(index).is_some_and(|token| token.text == text)
}

fn group_end(tokens: &[Token<'_>], start: usize) -> usize {
    let close = match tokens[start].text {
        "(" => ")",
        "[" => "]",
        "{" => "}",
        _ => panic!("expected delimiter"),
    };
    let mut index = start + 1;
    while index < tokens.len() {
        match tokens[index].text {
            text if text == close => return index + 1,
            "(" | "[" | "{" => index = group_end(tokens, index),
            _ => index += 1,
        }
    }
    panic!("unclosed source delimiter at {}", tokens[start].offset);
}

fn item_end(tokens: &[Token<'_>], mut index: usize) -> usize {
    while at(tokens, index, "#") && at(tokens, index + 1, "[") {
        index = group_end(tokens, index + 1);
    }
    let mut semicolon_item = None;
    while index < tokens.len() {
        match tokens[index].text {
            "use" | "type" | "static" if semicolon_item.is_none() => {
                semicolon_item = Some(true);
            }
            "const" if semicolon_item.is_none() => {
                // const fn 是带 body 的 item；const 的 fn-pointer 类型不是。
                semicolon_item = Some(
                    !tokens[index + 1..]
                        .iter()
                        .take_while(|token| !matches!(token.text, ":" | "=" | "{" | ";"))
                        .any(|token| token.text == "fn"),
                );
            }
            "fn" | "mod" | "impl" | "trait" | "struct" | "enum" | "union" | "macro_rules"
                if semicolon_item.is_none() =>
            {
                semicolon_item = Some(false);
            }
            ";" => return index + 1,
            "(" | "[" => {
                index = group_end(tokens, index);
                continue;
            }
            "{" => {
                index = group_end(tokens, index);
                if !semicolon_item.unwrap_or(false) {
                    return index + usize::from(at(tokens, index, ";"));
                }
                continue;
            }
            _ => {}
        }
        index += 1;
    }
    panic!("cfg(test) item has no end");
}

fn production_tokens(source: &str) -> Vec<Token<'_>> {
    let all = tokens(source);
    let attribute = ["#", "[", "cfg", "(", "test", ")", "]"];
    let mut result = Vec::new();
    let mut index = 0;
    while index < all.len() {
        if all
            .get(index..index + attribute.len())
            .is_some_and(|slice| {
                slice
                    .iter()
                    .zip(attribute)
                    .all(|(token, text)| token.text == text)
            })
        {
            index = item_end(&all, index + attribute.len());
        } else {
            result.push(all[index]);
            index += 1;
        }
    }
    result
}

fn forbidden(boundary: Boundary, name: &str) -> bool {
    match boundary {
        Boundary::Unit => name == "lower_frontend",
        Boundary::Support => matches!(
            name,
            "lower_frontend" | "unit_lower" | "unit_plan" | "llvm" | "model"
        ),
        Boundary::Ir => matches!(
            name,
            "lower_frontend"
                | "unit_lower"
                | "unit_plan"
                | "lowering_support"
                | "LoweringError"
                | "LoweringErrorKind"
                | "string_literal"
                | "decode_plain"
                | "decode_text"
        ),
    }
}

// use-tree 的 leaf 路径保留 self/*，as 后的名字无需解析：根模块导入本身即拒绝。
fn import_paths<'a>(tokens: &[Token<'a>], prefix: &[&'a str], out: &mut Vec<Vec<&'a str>>) {
    let mut index = 0;
    while index < tokens.len() {
        let mut path = prefix.to_vec();
        while index < tokens.len() {
            match tokens[index].text {
                "{" => {
                    let end = group_end(tokens, index);
                    import_paths(&tokens[index + 1..end - 1], &path, out);
                    index = end;
                    break;
                }
                "," | ";" | "as" => {
                    out.push(path);
                    while index < tokens.len() && !at(tokens, index, ",") {
                        index += 1;
                    }
                    break;
                }
                "::" => index += 1,
                _ => {
                    path.push(tokens[index].text);
                    index += 1;
                    if index == tokens.len() {
                        out.push(path.clone());
                    }
                }
            }
        }
        index += usize::from(at(tokens, index, ","));
    }
}

fn imports_unchecked_root(path: &[&str], module_depth: usize) -> bool {
    let mut index = 0;
    let mut depth = module_depth as isize;
    match path.first() {
        Some(&"crate") => {
            depth = -1;
            index += 1;
        }
        Some(&"super" | &"self") => {}
        _ => return false,
    }
    while let Some(&segment) = path.get(index) {
        match segment {
            "super" => depth -= 1,
            "self" => {}
            "ssa" if depth == -1 => depth = 0,
            "*" if index + 1 == path.len() => break,
            _ => return false,
        }
        index += 1;
    }
    // crate 的 glob/module alias 也能绕到 ssa，因此同样不能导入。
    depth <= 0
}

fn violations(path: &Path, source: &str) -> Vec<String> {
    let Some(boundary) = boundary(path) else {
        return Vec::new();
    };
    let tokens = production_tokens(source);
    let mut result = Vec::new();
    let mut depth = path.components().count() - usize::from(path.ends_with("mod.rs"));
    let mut scopes = Vec::new();
    for (index, token) in tokens.iter().enumerate() {
        let reason = if forbidden(boundary, token.text) {
            Some(format!("forbidden identifier `{}`", token.text))
        } else if boundary == Boundary::Ir && token.text == "use" {
            let end = tokens[index + 1..]
                .iter()
                .position(|token| token.text == ";")
                .map_or(tokens.len(), |end| index + 1 + end);
            let mut paths = Vec::new();
            import_paths(&tokens[index + 1..end], &[], &mut paths);
            paths
                .iter()
                .find(|path| imports_unchecked_root(path, depth))
                .map(|path| format!("SSA/crate root glob or module alias `{}`", path.join("::")))
        } else {
            None
        };
        if let Some(reason) = reason {
            let line = source[..token.offset]
                .bytes()
                .filter(|byte| *byte == b'\n')
                .count()
                + 1;
            result.push(format!("{}:{line}: {boundary:?}: {reason}", path.display()));
        }
        match token.text {
            "{" => {
                let module = index >= 2 && at(&tokens, index - 2, "mod");
                scopes.push(module);
                depth += usize::from(module);
            }
            "}" => depth -= usize::from(scopes.pop().expect("balanced production braces")),
            _ => {}
        }
    }
    result
}

fn scan_tree(root: &Path, dir: &Path, checked: &mut Vec<String>, failures: &mut Vec<String>) {
    let mut entries = fs::read_dir(dir)
        .expect("SSA source directory")
        .map(|entry| entry.expect("SSA directory entry").path())
        .collect::<Vec<_>>();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            scan_tree(root, &path, checked, failures);
        } else {
            let relative = path.strip_prefix(root).expect("SSA-relative source");
            if boundary(relative).is_some() {
                checked.push(relative.to_string_lossy().replace('\\', "/"));
                failures.extend(violations(
                    relative,
                    &fs::read_to_string(&path).expect("read SSA Rust source"),
                ));
            }
        }
    }
}

#[test]
fn production_lowering_dependencies_remain_directional() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/ssa");
    let mut checked = Vec::new();
    let mut failures = Vec::new();
    scan_tree(&root, &root, &mut checked, &mut failures);
    for required in [
        "unit_lower.rs",
        "unit_plan.rs",
        "model.rs",
        "types.rs",
        "verify.rs",
        "verify_ownership/closure.rs",
        "verify_ownership/field_exchange.rs",
        "verify_ownership/root_exchange.rs",
    ] {
        assert!(
            checked.iter().any(|path| path == required),
            "not scanned: {required}"
        );
    }
    assert!(
        failures.is_empty(),
        "lowering dependency boundary violations:\n{}",
        failures.join("\n")
    );
    assert!(
        checked
            .iter()
            .any(|path| path.starts_with("lowering_support")),
        "neutral support was not scanned"
    );
}

fn check(path: &str, source: &str) -> Vec<String> {
    violations(Path::new(path), source)
}

#[test]
fn dependency_guard_accepts_allowed_edges() {
    for (path, source) in [
        (
            "unit_lower.rs",
            "use super::{LoweringError, LoweringErrorKind, lowering_support::string_literal, model::Program, verify::verify_program};",
        ),
        (
            "unit_plan/runtime_layout.rs",
            "use crate::ssa::{LoweringError, lowering_support::error}; use lang_frontend::source::Span;",
        ),
        (
            "lowering_support.rs",
            "use lang_frontend::{ast::ExpressionId, source::Span}; mod string_literal;",
        ),
        (
            "lowering_support/string_literal.rs",
            "use super::{error, LoweringError, LoweringErrorKind};",
        ),
        (
            "types.rs",
            "use super::model::*; use crate::ssa::verify::{self, VerifyError};",
        ),
        (
            "verify_ownership/closure.rs",
            "use super::*; use crate::ssa::{model::Program, verify::VerifyError};",
        ),
        (
            "verify.rs",
            "mod inner { use super::*; mod deeper { use super::super::*; } }",
        ),
    ] {
        assert!(
            check(path, source).is_empty(),
            "allowed edge rejected: {path}: {source}"
        );
    }
}

#[test]
fn dependency_guard_rejects_reverse_imports_and_paths() {
    for path in [
        "unit_lower.rs",
        "unit_lower/nested/operation.rs",
        "unit_plan.rs",
        "unit_plan/runtime_layout.rs",
    ] {
        for source in [
            "use super::lower_frontend::{LoweringError, LoweringErrorKind};",
            "use crate::ssa::{lower_frontend as old_adapter};",
            "fn run() { super::super::lower_frontend::lower(); }",
            "use crate::ssa::r#lower_frontend::LoweringError;",
        ] {
            assert!(
                !check(path, source).is_empty(),
                "reverse edge escaped: {path}: {source}"
            );
        }
    }
    for forbidden in ["lower_frontend", "unit_lower", "unit_plan", "llvm", "model"] {
        let source = format!("use crate::ssa::{forbidden} as dependency;");
        assert!(!check("lowering_support/nested/helper.rs", &source).is_empty());
    }
}

#[test]
fn dependency_guard_ignores_comments_and_literals() {
    let source = r####"
        // use super::lower_frontend;
        /* lower_frontend /* unit_lower */ lowering_support */
        const TEXT: &str = "lower_frontend \\";
        const RAW: &str = r###"lower_frontend "## // still raw"###;
        const BYTES: &[u8] = br#"lower_frontend"#;
        const ESCAPED: &str = "a \" lower_frontend";
        const CHARACTER: char = '\"';
        const BYTE: u8 = b'\'';
        fn borrowed<'a>(x: &'a str) -> &'a str { x }
    "####;
    for path in ["unit_lower.rs", "lowering_support.rs", "verify.rs"] {
        assert!(
            check(path, source).is_empty(),
            "literal/comment rejected: {path}"
        );
    }
    let source = format!("{source}\nuse crate::ssa::lower_frontend::LoweringError;");
    assert!(!check("unit_lower.rs", &source).is_empty());
}

#[test]
fn dependency_guard_skips_only_the_cfg_test_item() {
    for item in [
        "mod tests { use super::lower_frontend; fn f() { lower_frontend::run(); } }",
        "#[allow(dead_code)] pub(crate) fn test_only() { lower_frontend::run(); }",
        "fn generic<const N: usize>() { lower_frontend::run(); }",
        "const fn test_only() { lower_frontend::run(); }",
        "const TEST: () = { lower_frontend::run(); };",
        "const CALLBACK: fn() = || { lower_frontend::run(); };",
        "use crate::ssa::lower_frontend::{LoweringError, LoweringErrorKind};",
        "mod external_tests;",
    ] {
        let allowed = format!("use super::model::Program; #[cfg(test)] {item}");
        assert!(
            check("unit_lower.rs", &allowed).is_empty(),
            "test item not skipped: {item}"
        );
        let production = format!("{allowed}\nuse super::lower_frontend::LoweringError;");
        let failures = check("unit_lower.rs", &production);
        assert_eq!(
            failures.len(),
            1,
            "production after test item was hidden: {item}"
        );
        assert!(failures[0].contains(":2:"));
    }
}

#[test]
fn dependency_guard_scans_nested_ir_and_excludes_test_files() {
    for path in [
        "model/nested/item.rs",
        "types/nested/item.rs",
        "verify_types/nested/item.rs",
        "verify_ownership/closure.rs",
        "verify_ownership/field_exchange.rs",
        "verify_ownership/root_exchange.rs",
    ] {
        assert!(
            !check(path, "use crate::ssa::lowering_support::string_literal;").is_empty(),
            "IR not checked: {path}"
        );
        assert!(!check(path, "use crate::ssa::unit_plan::Plan;").is_empty());
    }
    for path in [
        "unit_lower/operation_tests.rs",
        "unit_plan_tests.rs",
        "lowering_support/helper_tests.rs",
        "verify_ownership/field_exchange_tests.rs",
        "lower_frontend.rs",
        "render.rs",
    ] {
        assert!(
            check(path, "use crate::ssa::lower_frontend::LoweringError;").is_empty(),
            "outside selected production files: {path}"
        );
    }
}

#[test]
fn dependency_guard_rejects_ir_root_facades() {
    for identifier in [
        "LoweringError",
        "LoweringErrorKind",
        "string_literal",
        "decode_plain",
        "decode_text",
    ] {
        for path in [
            "model.rs",
            "types.rs",
            "verify.rs",
            "verify_ownership/closure.rs",
        ] {
            let source = format!("use crate::ssa::{identifier} as hidden;");
            assert!(
                !check(path, &source).is_empty(),
                "root facade escaped: {path}: {source}"
            );
        }
    }
}

#[test]
fn dependency_guard_rejects_ir_root_globs_and_aliases() {
    for source in [
        "use super::*;",
        "use super::{*};",
        "use crate::ssa::*;",
        "use crate::{ssa::{*}};",
        "use crate::ssa as root;",
        "use crate::{ssa as root};",
        "use crate::ssa::{self as root};",
        "use crate::ssa;",
        "use super as root;",
        "use super::{self as root};",
        "use crate as root;",
        "use crate::*;",
    ] {
        assert!(
            !check("model.rs", source).is_empty(),
            "root import escaped: {source}"
        );
    }
    assert!(!check("verify_ownership/closure.rs", "use super::super::*;").is_empty());
    assert!(!check("model.rs", "mod inner { use super::super as root; }").is_empty());
    assert!(check("verify_ownership/mod.rs", "use super::model::*;").is_empty());
    assert!(!check("verify_ownership/mod.rs", "use super::*;").is_empty());
}
