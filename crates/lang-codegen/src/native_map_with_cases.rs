//! 普通 scoped callback 源码与 native 输出。
pub(crate) fn cases() -> Vec<(&'static str, String, &'static [u8])> {
    let mut cases = Vec::new();
    for (name, text, expected) in crate::native_map_require_cases::cases() {
        if name == "nullable" {
            continue;
        }
        let source = text.replace("{ borrow val item = m.requireValue(key); observe(item); println(key) }", "if (m.withValue(key, { item -> observe(item); println(key); return })) {} else { error(\"missing key\") }")
   .replace("{ borrow val item = m.requireValue(\"key\"); observe(item) }", "if (m.withValue(\"key\".clone(), { item -> observe(item); return })) {} else { error(\"missing key\") }");
        cases.push((name, source, expected));
    }
    cases.push(("missing", "fun entry() { var m = mutableMapOf<String, String>(); if (m.withValue(\"missing\".clone(), { item -> error(\"must not call\") })) { error(\"must be false\") }; m.put(\"after\".clone(), \"valid\".clone()); println(\"missing\") }".into(), b"missing\n"));
    cases.push(("capture", "fun entry() { var m = mutableMapOf<String,String>(); m.put(\"key\".clone(), \"value\".clone()); val label = \"captured\".clone(); val key=\"key\".clone(); if (m.withValue(key.clone(), { item -> println(label); println(item); return })) {println(key)}; m.put(\"after\".clone(), \"valid\".clone()); println(label) }".into(), b"captured\nvalue\nkey\ncaptured\n"));
    cases.push(("nullable", "class Token(val n:Int)\nfun observe(item:Token?) { if (item == null) { println(\"null\") } else { println(\"value\") } }\nfun entry() { var m=mutableMapOf<String,Token?>(); val empty:Token?=null; m.put(\"empty\".clone(),empty); val full:Token?=Token(7); m.put(\"full\".clone(),full); if (m.withValue(\"missing\", {item -> observe(item)})) {error(\"must be false\")}; if (m.withValue(\"empty\", {item -> observe(item)})) {println(\"found-null\")}; if (m.withValue(\"full\", {item -> observe(item)})) {println(\"found-value\")}; m.put(\"after\".clone(),null) }".into(), b"null\nfound-null\nvalue\nfound-value\n"));
    cases.push(("readonly-missing", "fun inspect(m:Map<String,String>) {if(m.withValue(\"missing\",{item -> error(\"must not call\")})) {error(\"must be false\")};println(\"readonly-missing\")}\nfun entry() {val m=mapOf<String,String>();inspect(m)}".into(),b"readonly-missing\n"));
    cases
}
