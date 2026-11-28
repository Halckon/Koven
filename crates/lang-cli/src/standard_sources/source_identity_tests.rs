//! 标准宿主身份避让只作用于完整键，不改变用户路径或来源权限。
use super::*;
use lang_frontend::{
    analysis::{UnitNameAnalysisError, analyze_unit_names},
    name_resolution::CompilationUnitInputError,
    type_checking::standard_environments,
};

#[test]
fn standard_source_key_collision_preserves_user_diagnostics_and_authority() {
    for (path, package) in [
        (RANGE_PATH, "koven.algorithms"),
        (RESULT_PATH, "koven.core"),
        (TEXT_PATH, "koven.text"),
        (DISPLAY_PATH, "koven.core"),
    ] {
        let mut sources = SourceMap::new();
        let text = format!("package {package}\nfun probe(): Unit {{ missing }}\n");
        let start = text.find("missing").unwrap();
        let display = format!("koven-std/{path}");
        let user = sources.add_source(&display, &text).unwrap();
        let original = UnitSourceDescriptor::new("koven-std", path, user);
        let mut descriptors = vec![original.clone()];
        let (mut names, mut types) = standard_environments();
        append_standard_sources(&mut sources, &mut descriptors, &mut names, &mut types).unwrap();
        assert_eq!(descriptors[0], original);
        for (standard, (path, text)) in descriptors[1..].iter().zip([
            (RANGE_PATH, RANGE_TEXT),
            (RESULT_PATH, RESULT_TEXT),
            (TEXT_PATH, TEXT_TEXT),
        ]) {
            assert_ne!(standard.source_id(), user);
            assert_eq!(standard.logical_path(), Some(path));
            assert_eq!(sources.source_text(standard.source_id()).unwrap(), text);
        }

        assert!(!types.is_authorized_range_source(user));
        assert!(!types.is_authorized_range_extension_source(user));
        assert!(!types.is_authorized_text_source(user));
        assert_eq!(names.builtin_source(), Some(descriptors[2].source_id()));
        assert!(types.is_authorized_range_source(descriptors[1].source_id()));
        assert!(types.is_authorized_range_extension_source(descriptors[1].source_id()));
        assert!(types.is_authorized_text_source(descriptors[3].source_id()));
        // 先通过真实 index/name 链，旧实现在此返回 DuplicateSourceKey。
        let snapshot = analyze_unit_names(sources, descriptors, names).unwrap();
        let [diagnostic] = snapshot.names().diagnostics() else {
            panic!("one unresolved user reference");
        };
        assert_eq!(diagnostic.code().to_string(), "L0080");
        assert_eq!(
            diagnostic.primary_span(),
            snapshot.sources().span(user, start, start + 7).unwrap()
        );
        assert_eq!(snapshot.sources().source_name(user).unwrap(), display);
        for standard in &snapshot.descriptors()[1..] {
            assert_eq!(standard.root_identity(), Some("koven-std-1"));
        }
        let standard = snapshot
            .descriptors()
            .iter()
            .skip(1)
            .find(|item| item.logical_path() == Some(path))
            .unwrap();
        assert_eq!(
            snapshot
                .sources()
                .source_name(standard.source_id())
                .unwrap(),
            format!("{display} [standard source 1]")
        );
    }
}

#[test]
fn standard_source_root_selection_is_bounded_and_input_order_independent() {
    for reverse in [false, true] {
        let mut sources = SourceMap::new();
        let mut descriptors = Vec::new();
        for (root, path, package) in [
            ("koven-std", RESULT_PATH, "koven.core"),
            ("koven-std-1", TEXT_PATH, "koven.text"),
            ("koven-std-2", "other.ko", ""),
        ] {
            let text = if package.is_empty() {
                String::new()
            } else {
                format!("package {package}\n")
            };
            let source = sources.add_source(format!("{root}/{path}"), text).unwrap();
            descriptors.push(UnitSourceDescriptor::new(root, path, source));
        }
        if reverse {
            descriptors.reverse();
        }
        let originals = descriptors.clone();
        let (mut names, mut types) = standard_environments();
        append_standard_sources(&mut sources, &mut descriptors, &mut names, &mut types).unwrap();
        assert_eq!(&descriptors[..3], &originals);
        for standard in &descriptors[3..] {
            assert_eq!(standard.root_identity(), Some("koven-std-2"));
        }
        let snapshot = analyze_unit_names(sources, descriptors, names).unwrap();
        assert!(snapshot.names().diagnostics().is_empty());
    }
}

#[test]
fn noncolliding_user_path_keeps_original_standard_root() {
    let mut sources = SourceMap::new();
    let user = sources.add_source("user", "").unwrap();
    let mut descriptors = vec![UnitSourceDescriptor::new("koven-std", "other.ko", user)];
    let (mut names, mut types) = standard_environments();
    append_standard_sources(&mut sources, &mut descriptors, &mut names, &mut types).unwrap();
    for standard in &descriptors[1..] {
        assert_eq!(standard.root_identity(), Some("koven-std"));
    }
    let snapshot = analyze_unit_names(sources, descriptors, names).unwrap();
    assert!(snapshot.names().diagnostics().is_empty());
}

#[test]
fn standard_source_key_avoidance_does_not_repair_duplicate_user_keys() {
    let mut sources = SourceMap::new();
    let mut descriptors = Vec::new();
    for display in ["first", "second"] {
        let user = sources.add_source(display, "package koven.core\n").unwrap();
        descriptors.push(UnitSourceDescriptor::new("koven-std", RESULT_PATH, user));
    }
    let (mut names, mut types) = standard_environments();
    append_standard_sources(&mut sources, &mut descriptors, &mut names, &mut types).unwrap();
    assert!(matches!(analyze_unit_names(sources, descriptors, names),
        Err(UnitNameAnalysisError::Input(CompilationUnitInputError::DuplicateSourceKey { root, logical_path }))
        if root == "koven-std" && logical_path == RESULT_PATH));
}
