use lang_frontend::{
    name_resolution::SourceUnitInput, source::SourceMap, type_checking::standard_environments,
};

use crate::llvm::render_verified_program_with_debug;

use super::{
    unit_lower::lower_scalar_unit_with_entry,
    unit_lower_test_support::{analyze, declaration, parsed},
};

#[test]
fn lowers_compilation_unit_to_deterministic_multisource_debug_llvm() {
    let mut sources = SourceMap::new();
    let (provider_source, provider) = parsed(
        &mut sources,
        "p/provider.ko",
        "package p\n\
         fun make(number: Int): String = \"provider\"\n\
         fun inspect(message: String): Unit {}",
    );
    let (consumer_source, consumer) = parsed(
        &mut sources,
        "q/consumer.ko",
        "package q\n\
         import p.make as build\n\
         fun entry(): Unit {\n\
             val offset = 2\n\
             val action: move (borrow Int) -> String = move { item -> build(item + offset) }\n\
             val message = action(3)\n\
             val seen = p.inspect(message)\n\
         }",
    );
    let inputs = [
        SourceUnitInput::new("root", "p/provider.ko", provider_source, &provider),
        SourceUnitInput::new("root", "q/consumer.ko", consumer_source, &consumer),
    ];
    let reversed = [inputs[1], inputs[0]];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);
    let (reverse_names, reverse_typed, reverse_owned) =
        analyze(&sources, &reversed, &name_environment, &type_environment);
    let (forward, forward_entry) = lower_scalar_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &type_environment,
        &typed,
        &owned,
        declaration(&names, "q", "entry"),
    )
    .expect("compilation unit lowers to verified SSA");
    let (backward, backward_entry) = lower_scalar_unit_with_entry(
        &sources,
        &reversed,
        &reverse_names,
        &type_environment,
        &reverse_typed,
        &reverse_owned,
        declaration(&reverse_names, "q", "entry"),
    )
    .expect("reversed compilation unit lowers to verified SSA");

    let forward = render_verified_program_with_debug(&forward, &sources, forward_entry)
        .expect("compilation-unit SSA lowers to verified LLVM with debug metadata");
    let backward = render_verified_program_with_debug(&backward, &sources, backward_entry)
        .expect("input permutation preserves LLVM and debug identities");

    assert_eq!(forward, backward);
    let provider_file = metadata_id(&forward, "!DIFile(filename: \"p/provider.ko\"");
    let consumer_file = metadata_id(&forward, "!DIFile(filename: \"q/consumer.ko\"");
    let make_metadata = metadata_line(&forward, "!DISubprogram(name: \"koven.p.make.d");
    let inspect_metadata = metadata_line(&forward, "!DISubprogram(name: \"koven.p.inspect.d");
    let entry_metadata = metadata_line(&forward, "!DISubprogram(name: \"koven.q.entry.d");
    let thunk_metadata = metadata_line(&forward, "!DISubprogram(name: \"unit.lambda.");
    assert!(make_metadata.contains(&format!("file: {provider_file}")));
    assert!(inspect_metadata.contains(&format!("file: {provider_file}")));
    assert!(entry_metadata.contains(&format!("file: {consumer_file}")));
    assert!(thunk_metadata.contains(&format!("file: {consumer_file}")));
    let make_scope = metadata_line_id(make_metadata);
    let entry_scope = metadata_line_id(entry_metadata);
    assert!(forward.lines().any(|line| {
        line.contains("!DILocation(line: 2, column: 1")
            && line.contains(&format!("scope: {make_scope}"))
    }));
    assert!(forward.lines().any(|line| {
        line.contains("!DILocation(line: 6, column: 15")
            && line.contains(&format!("scope: {entry_scope}"))
    }));
    assert_eq!(forward.matches("!DICompileUnit(").count(), 1);
    assert_eq!(forward.matches("!DISubprogram(").count(), 4);

    let make_definition = ir_line(&forward, "define internal", "koven.p.make.d");
    let thunk_definition = ir_line(&forward, "define internal", ".thunk(");
    let make_call = ir_line(&forward, "call %koven.string.t", "koven.p.make.d");
    let thunk_call = ir_line(&forward, "call %koven.string.t", ".thunk(");
    let inspect_call = ir_line(&forward, "call void", "koven.p.inspect.d");
    assert!(make_definition.contains("(ptr %l0)"));
    assert!(thunk_definition.contains("(ptr %l0, ptr %l1)"));
    assert!(make_call.contains("(ptr %p0)"));
    assert!(thunk_call.contains("(ptr %invoke.environment.storage, ptr %p0)"));
    assert!(inspect_call.contains("(ptr %p1)"));

    let string_type = forward
        .lines()
        .find_map(|line| {
            line.strip_prefix("%koven.string.")?
                .split_once(" =")
                .map(|(id, _)| id)
        })
        .expect("String LLVM type is declared");
    let string_drop = format!("call void @koven.drop.{string_type}(");
    assert_eq!(forward.matches(&string_drop).count(), 1);
    assert_eq!(forward.matches("call void @free(ptr").count(), 1);
    assert!(
        forward.find(inspect_call).expect("inspect call offset")
            < forward.find(&string_drop).expect("String drop offset")
    );
}

fn metadata_line<'a>(ir: &'a str, marker: &str) -> &'a str {
    ir.lines()
        .find(|line| line.contains(marker))
        .unwrap_or_else(|| panic!("missing LLVM metadata marker: {marker}"))
}

fn metadata_id<'a>(ir: &'a str, marker: &str) -> &'a str {
    metadata_line_id(metadata_line(ir, marker))
}

fn metadata_line_id(line: &str) -> &str {
    line.split_once(" =")
        .map(|(id, _)| id)
        .expect("LLVM metadata line has an id")
}

fn ir_line<'a>(ir: &'a str, first: &str, second: &str) -> &'a str {
    ir.lines()
        .find(|line| line.contains(first) && line.contains(second))
        .unwrap_or_else(|| panic!("missing LLVM line containing {first:?} and {second:?}"))
}
