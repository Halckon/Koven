//! Phase 1 Parser 矩阵共享的合法完整文件 corpus。

#[derive(Clone, Copy)]
pub(crate) struct GrammarCase {
    pub(crate) name: &'static str,
    pub(crate) source: &'static str,
}

pub(crate) const GRAMMAR_CASES: &[GrammarCase] = &[
    GrammarCase {
        name: "file header",
        source: "package demo.core; import lib.*; val answer = 42",
    },
    GrammarCase {
        name: "variables and constant",
        source: "public val answer: Int = 42; var counter: Int = 0; const val limit: Int = 10",
    },
    GrammarCase {
        name: "generic function",
        source: "fun <T: Copyable> id(borrow item: T): T = item",
    },
    GrammarCase {
        name: "function type",
        source: "val handler: move (borrow Int, inout String) -> Unit = target",
    },
    GrammarCase {
        name: "typed named mode call",
        source: "val result = service.send<Int>(name = input, &target)",
    },
    GrammarCase {
        name: "move lambda",
        source: "val combine = move { left, right -> left + right }",
    },
    GrammarCase {
        name: "local destructuring",
        source: "fun usePair() { val (first, second) = pair }",
    },
    GrammarCase {
        name: "if expression",
        source: "val choice = if (ready) yes else no",
    },
    GrammarCase {
        name: "when expression",
        source: "val choice = when (input) { is Type -> yes; !is Other -> no; else -> fallback }",
    },
    GrammarCase {
        name: "loop family",
        source: "fun loops() { while (ready) { continue } for (item in items) { break } loop { return } }",
    },
    GrammarCase {
        name: "postfix chain",
        source: "val result = source?.member!!?",
    },
    GrammarCase {
        name: "ordinary class",
        source: "public class Box<T: Copyable>(private val item: T, var count: Int): Printable { override fun show(): Unit {} }",
    },
    GrammarCase {
        name: "value class",
        source: "value class Meters(val amount: Int)",
    },
    GrammarCase {
        name: "interface companion",
        source: "interface Printable { fun show(): Unit; companion object { const val NAME: String = \"printable\" } }",
    },
    GrammarCase {
        name: "enum variants and member",
        source: "enum class Result<T> { Ok(payload: T), Error(message: String); fun isOk(): Boolean = true }",
    },
    GrammarCase {
        name: "named object",
        source: "object Config { const val VERSION: Int = 1; fun load(): Unit {} }",
    },
    GrammarCase {
        name: "interface delegation",
        source: "class Screen(val renderer: Renderer): Draw by renderer, Resettable {}",
    },
    GrammarCase {
        name: "operator hierarchy",
        source: "val result = target = a ?: b || c && d == e + f * g",
    },
    GrammarCase {
        name: "cast contains and type test",
        source: "val cast = input as? Type?; val contained = item !in items; val tested = item !is Type",
    },
    GrammarCase {
        name: "index call and reference",
        source: "val result = array[index].member(argument)::ref",
    },
    GrammarCase {
        name: "super member",
        source: "val result = super<Logger>.log(message)",
    },
    GrammarCase {
        name: "unicode lexical owners",
        source: r#"val text = "前${call('界', "内${x}")}后" /*尾*/"#,
    },
];
