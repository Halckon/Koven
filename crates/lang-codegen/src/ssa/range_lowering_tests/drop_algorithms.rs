//! Healthy source checks for std range algorithms; no IR mutation.
use super::*;

#[test]
fn range_drop_algorithms_single_and_unit_share_the_descriptor_abi() {
    let consumer = "fun read(view:View<String>):Unit{for(item in view){println(item)}}\nfun consume(own root:List<String>):Unit{}\nfun main():Unit{val root=listOf(\"a\",\"b\",\"c\");borrow val parent=drop(root,1);borrow val child=parent.dropLast(1);borrow val sibling=dropLast(parent,0);borrow val empty=parent.drop(2147483647);read(child);read(sibling);read(empty);consume(root)}";
    for program in [
        single(consumer),
        unit(&format!(
            "package app\nimport koven.algorithms.drop\nimport koven.algorithms.dropLast\n{consumer}"
        )),
    ] {
        assert!(
            program.modules[0]
                .functions
                .iter()
                .flat_map(|f| &f.instructions)
                .any(|i| matches!(i.operation, Operation::RangeCall { .. }))
        );
        crate::llvm::render_verified_program(&program).unwrap();
    }
}

#[test]
fn range_drop_algorithms_reuse_arbitrary_named_dynamic_boundary_producers() {
    let trusted = "fun <T> window(source:List<T>,begin:Int,end:Int):View<T> from source=rangeView(source,begin,end)\nfun <T> window(source:View<T>,begin:Int,end:Int):View<T> from source=rangeView(source,begin,end)";
    let consumer = "fun read(view:View<String>):Unit{for(item in view){println(item)}}\nfun main():Unit{val root=listOf(\"a\",\"b\",\"c\",\"d\");borrow val parent=window(root,1,4);borrow val child=window(parent,1,2);read(dropLast(child,0));read(drop(parent,1))}";
    for program in [single(&format!("{trusted}\n{consumer}")), unit_with_provider_entry(&format!("package app\nimport koven.algorithms.window\nimport koven.algorithms.drop\nimport koven.algorithms.dropLast\n{consumer}"), trusted).0] {
        crate::llvm::render_verified_program(&program).unwrap();
    }
}

#[test]
fn range_drop_algorithms_single_and_reversed_unit_run_unmodified_llvm_natively() {
    use std::{fs, process::Command};
    let trusted = "fun <T> window(source:List<T>,begin:Int,end:Int):View<T> from source=rangeView(source,begin,end)";
    let consumer = "class Item(val text:String){deinit(){println(this.text)}}\nfun read(view:View<Item>):Unit{for(item in view){println(item.text)}}\nfun consume(own root:List<Item>):Unit{println(\"consume\")}\nfun main():Unit{val root=listOf(Item(\"a\"),Item(\"b\"),Item(\"c\"),Item(\"d\"));borrow val parent=window(root,1,4);borrow val child=parent.dropLast(1);borrow val last=drop(child,1);read(last);read(child);consume(root);println(\"done\")}";
    for (index, (program, entry)) in [
        single_with_entry(&format!("{trusted}\n{consumer}")),
        unit_with_provider_order(&format!("package app\nimport koven.algorithms.window\nimport koven.algorithms.drop\nimport koven.algorithms.dropLast\n{consumer}"), trusted, true),
    ].into_iter().enumerate() {
        let directory = std::env::temp_dir().join(format!("koven-drop-native-{}-{index}", std::process::id()));
        fs::create_dir_all(&directory).unwrap();
        let ir = directory.join("program.ll");
        let executable = directory.join("program");
        let llvm = crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
        fs::write(&ir, llvm).unwrap();
        let linked = Command::new(crate::test_support::ir_clang()).arg(&ir).arg("-o").arg(&executable).output().unwrap();
        assert!(linked.status.success(), "{linked:?}");
        let run = Command::new(&executable).output().unwrap();
        assert_eq!(run.status.code(), Some(0), "{run:?}");
        assert_eq!(run.stdout, b"c\nb\nc\nconsume\nd\nc\nb\na\ndone\n");
        assert!(run.stderr.is_empty(), "{run:?}");
        fs::remove_dir_all(directory).unwrap();
    }
}
