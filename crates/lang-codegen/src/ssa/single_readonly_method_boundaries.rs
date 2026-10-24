//! Scope controls use only original frontend-produced source facts.
use super::*;
use lang_frontend::{
    name_resolution::resolve_names, ownership_checking::check_ownership, type_checking::check_types,
};

// These old negative controls intentionally retain the independent legacy path.
// They are not positive MemoryDocument consumers and do not select a fallback.
fn lower_legacy_single(text: &str) -> (SourceMap, Result<(Program, FunctionId), LoweringError>) {
    let mut sources = SourceMap::new();
    let (_, parsed) = super::super::unit_lower_test_support::parsed(
        &mut sources,
        "app/readonly-method.ko",
        &format!("package app\n{text}"),
    );
    let (environment, types) = standard_environments();
    let names = resolve_names(&sources, &parsed, &environment).unwrap();
    assert!(names.diagnostics().is_empty(), "{:?}", names.diagnostics());
    let typed = check_types(&sources, &parsed, &names, &types).unwrap();
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    let owned = check_ownership(&sources, &parsed, &names, &typed).unwrap();
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let entry = names
        .symbols()
        .iter()
        .find(|s| s.name() == "entry")
        .unwrap()
        .id();
    let result = super::super::lower_frontend::orchestrate::lower_scalar_file_with_entry(
        &sources, &parsed, &names, &typed, &owned, entry,
    );
    (sources, result)
}

fn rejected(source: &str, fragment: &str) {
    let (sources, result) = lower_legacy_single(source);
    let error = result
        .err()
        .expect("unselected single receiver slice remains rejected");
    assert_eq!(
        error.kind,
        LoweringErrorKind::UnsupportedNode,
        "{source}: {error:?}"
    );
    assert_eq!(
        error.span.and_then(|span| sources.slice(span).ok()),
        Some(fragment),
        "{source}: {error:?}"
    );
}

#[test]
fn readonly_keeps_receiver_and_result_boundaries() {
    for (declaration, call) in [
        (
            "value class Item(val n:Int) { own fun read():Int=this.n }",
            "item.read()",
        ),
        (
            "value class Item(var n:Int) { inout fun read():Int=this.n }",
            "item.read()",
        ),
        (
            "value class Item(val n:Int) { fun <T> read(own value:T):T=value }",
            "item.read(7)",
        ),
        (
            "value class Item<T>(val n:T) { fun read():Int=7 }",
            "item.read()",
        ),
        (
            "value class Item(val n:Int) { fun read():Long=7 }",
            "item.read()",
        ),
        (
            "value class Item(val n:Int) { fun read():Unit {} }",
            "item.read()",
        ),
        (
            "class Item(val n:Int) { fun read():Int=this.n }",
            "item.read()",
        ),
    ] {
        rejected(
            &format!("{declaration}\nfun entry() {{ var item=Item(7); {call} }}"),
            call,
        );
    }
}

#[test]
fn readonly_keeps_string_field_operations_outside_new_this_slice() {
    for body in [
        "println(this.text); return 7",
        "println(this.text.clone()); return 7",
    ] {
        rejected(
            &format!(
                "value class Item(val text:String) {{ fun read():Int {{ {body} }} }}\nfun entry() {{ val item=Item(\"text\"); item.read() }}"
            ),
            "this.text",
        );
    }
}

#[test]
fn readonly_keeps_borrow_result_receiver_boundary() {
    rejected(
        "value class Item(val n:Int) { fun read():Int=this.n }\nfun view(item:Item):borrow Item from item=item\nfun entry() { val item=Item(7); borrow val alias=view(item); alias.read() }",
        "alias.read()",
    );
}

#[test]
fn readonly_keeps_grouped_borrow_result_and_field_receiver_boundaries() {
    rejected(
        "value class Item(val n:Int) { fun read():Int=this.n }\nfun view(item:Item):borrow Item from item=item\nfun entry() { val item=Item(7); borrow val alias=view(item); ((alias)).read() }",
        "((alias)).read()",
    );
    rejected(
        "value class Item(val n:Int) { fun read():Int=this.n }\nvalue class Outer(val item:Item)\nfun entry() { val outer=Outer(Item(7)); outer.item.read() }",
        "outer.item.read()",
    );
}

fn nullable_receiver(receiver: &str) {
    let call = format!("{receiver}.read()");
    rejected(
        &format!(
            "value class Item(val n:Int, val text:String) {{ fun read():Int=this.n }}\nfun entry() {{ var values=mutableMapOf<Int,Item>(); values.put(1,Item(7,\"text\")); val removed=values.remove(1); if(removed!=null) {{ {call} }} }}"
        ),
        &call,
    );
}

#[test]
fn readonly_keeps_nullable_receiver_boundary() {
    nullable_receiver("removed");
}

#[test]
fn readonly_keeps_grouped_nullable_receiver_boundary() {
    nullable_receiver("((removed))");
}

#[test]
fn readonly_keeps_iteration_receiver_boundary() {
    rejected(
        "value class Item(val n:Int) { fun read():Int=this.n }\nfun entry() { val items=listOf(Item(7)); for(item in items) { item.read() } }",
        "item.read()",
    );
}
