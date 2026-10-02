use super::*;

#[test]
fn receiver_two_phase_nested_readonly_native_source_order() {
    for kind in ["class", "value class"] {
        let analysis = analyze_sources(
            &"package p\nCLASS Cell(var item: Int) {\n\
         fun read(): Int { println(\"read\")\nreturn item }\n\
         inout fun set(first: Int, own second: Int): Unit {\n\
         println(\"set\")\nitem = first + second }\n\
         inout fun relay(): Unit { val done = set(0, inspect((this))) }\n}\n\
         fun inspect(cell: Cell): Int { println(\"inspect\")\nreturn cell.read() + 1 }\n\
         fun make(): Cell { println(\"make\")\nreturn Cell(20) }\n\
         fun argument(own value: Int): Int { println(\"argument\")\nreturn value }"
                .replace("CLASS", kind),
            "package q\nimport p.Cell\nfun entry(): Unit {\n\
         var cell = p.make()\n\
         val ignored = cell.set(second = p.argument(cell.read() + 1), first = p.argument(cell.read()))\n\
         if (cell.read() != 41) { error(\"wrong result\") }\n\
         val relayed = cell.relay()\n\
         if (cell.read() != 42) { error(\"wrong reborrow result\") }\n}",
        );
        let inputs = analysis.inputs();
        let directory = TestDirectory::create();
        let object = directory.join("two-phase.o");
        let executable = directory.join("two-phase");
        emit_native_unit_object(
            &analysis.sources,
            &inputs,
            &analysis.names,
            &analysis.environment,
            &analysis.typed,
            &analysis.owned,
            analysis.declaration("q", "entry"),
            &object,
        )
        .expect("two-phase receiver must emit native object");
        let linked = Command::new(crate::test_support::clang())
            .arg(&object)
            .arg("-o")
            .arg(&executable)
            .output()
            .unwrap();
        assert!(linked.status.success(), "{linked:?}");
        let run = Command::new(&executable).output().unwrap();
        assert!(run.status.success(), "{run:?}");
        assert_eq!(
            run.stdout,
            b"make\nread\nargument\nread\nargument\nset\nread\ninspect\nread\nset\nread\n"
        );
        assert_no_sibling_temporary(&directory.0);
    }
}
