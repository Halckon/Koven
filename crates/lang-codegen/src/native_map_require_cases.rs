//! 普通源码的 requireValue 真槽位借用与缺失错误路径。
pub(crate) fn cases() -> Vec<(&'static str, &'static str, &'static [u8])> {
    vec![
        (
            "string",
            r#"fun observe(item: String) { println(item) }
fun entry() {
 var m = mutableMapOf<String, String>()
 val key = "key".clone()
 m.put(key.clone(), "value".clone())
 { borrow val item = m.requireValue(key); observe(item); println(key) }
 m.put("other".clone(), "new".clone())
}"#,
            b"value\nkey\n",
        ),
        (
            "inline",
            r#"value class Packet(val text: String)
fun observe(item: Packet) { println(item.text.clone()) }
fun entry() {
 var m = mutableMapOf<String, Packet>()
 m.put("key".clone(), Packet("inline".clone()))
 { borrow val item = m.requireValue("key"); observe(item) }
 m.put("key".clone(), Packet("replacement".clone()))
}"#,
            b"inline\n",
        ),
        (
            "resource",
            r#"class Resource(val n: Int) { deinit() { if (n == 1) { println("drop-one") } else { println("drop-two") } } }
fun observe(item: Resource) { if (item.n == 1) { println("resource") } else { error("wrong source") } }
fun entry() {
 var m = mutableMapOf<String, Resource>()
 m.put("key".clone(), Resource(1))
 { borrow val item = m.requireValue("key"); observe(item) }
 m.put("key".clone(), Resource(2))
}"#,
            b"resource\ndrop-one\ndrop-two\n",
        ),
        (
            "nullable",
            r#"class Token(val n: Int)
fun observe(item: Token?) { println("nullable-slot") }
fun entry() {
 var m = mutableMapOf<String, Token?>()
 val empty: Token? = null
 m.put("empty".clone(), empty)
 val full: Token? = Token(7)
 m.put("full".clone(), full)
 { borrow val item = m.requireValue("empty"); observe(item) }
 { borrow val item = m.requireValue("full"); observe(item) }
 m.put("after".clone(), null)
}"#,
            b"nullable-slot\nnullable-slot\n",
        ),
    ]
}
pub(crate) const MISSING: &str = "fun entry() { val m = mapOf<String, String>(); borrow val item = m.requireValue(\"missing\"); println(item) }";
