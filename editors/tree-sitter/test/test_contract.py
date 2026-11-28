"""Exercise the real CLI tree, including text/ranges that corpus S-expressions omit."""

import os
from pathlib import Path
import subprocess
import tempfile
import unittest
import xml.etree.ElementTree as ET

GRAMMAR = Path(__file__).resolve().parents[1]
CLI = os.environ.get("TREE_SITTER", "tree-sitter")


class EditorContractTests(unittest.TestCase):
    def parse(self, source, valid=True):
        with tempfile.TemporaryDirectory(prefix="koven-editor-") as directory:
            path = Path(directory) / "probe.ko"
            path.write_text(source)
            result = subprocess.run([CLI, "parse", "--xml", str(path)], cwd=GRAMMAR,
                                    text=True, capture_output=True, check=False)
        self.assertEqual(result.returncode, 0 if valid else 1, result.stdout + result.stderr)
        start = result.stdout.index("<?xml")
        end = result.stdout.index("</sources>") + len("</sources>")
        xml_text = result.stdout[start:end].replace("\x0c", "")
        source_node = ET.fromstring(xml_text).find("source")
        self.assertIsNotNone(source_node)
        tree = source_node.find("source_file")
        if valid:
            self.assertIsNotNone(tree)
            self.assertEqual(list(tree.iter("ERROR")), [])
        else:
            if tree is None:
                tree = source_node.find("ERROR")
            self.assertIsNotNone(tree)
            self.assertTrue(list(tree.iter("ERROR")), result.stdout)
        return tree

    def test_parameter_modes_and_same_spelling_names(self):
        for mode in ("own", "borrow", "inout"):
            with self.subTest(mode=mode):
                tree = self.parse(f"fun f({mode} item: Int, {mode}: Int): Int = item\n")
                self.assertEqual(len(list(tree.iter("parameter_mode"))), 1)
                tree = self.parse(f"val f: ({mode} Int, {mode}) -> Int = handler\n")
                self.assertEqual(len(list(tree.iter("parameter_mode"))), 1)
                tree = self.parse(f"class Holder {{ {mode} /* receiver */ fun f(): Int = 1 }}\n"
                                  f"val f: ({mode} /* function */ (Int) -> Int) -> Int = handler\n")
                self.assertEqual(len(list(tree.iter("parameter_mode"))), 2)

    def test_soft_words_keep_ordinary_name_and_call_positions(self):
        for word in ("value", "loop", "own", "borrow", "inout", "move", "by", "to"):
            with self.subTest(word=word):
                tree = self.parse(f"fun {word}({word}: Int): Int = {word}\n"
                                  f"val result = {word}({word}) + cell.{word}()\n")
                self.assertEqual(len(list(tree.iter("call_expression"))), 2)
                self.assertEqual(list(tree.iter("parameter_mode")), [])

    def test_contextual_constructs_have_their_actual_nodes(self):
        tree = self.parse("value class Point(val value: Int)\n"
                          "class Holder { own fun take(): Int = 1 }\n"
                          "val f: move (own Int) -> Int = move { item -> item }\n"
                          "fun run(): Unit { loop { break } }\n"
                          "val pair = 1 to 2\nclass Adapter(val inner: Shape): Shape by inner\n")
        for tag, count in (("value_class_declaration", 1), ("parameter_mode", 2),
                           ("loop_statement", 1), ("lambda_expression", 1), ("to_operator", 1), ("delegation_keyword", 1)):
            self.assertEqual(len(list(tree.iter(tag))), count, tag)
        closure = tree.find(".//lambda_expression")
        self.assertIn("move", closure.text)

    def test_named_argument_precedes_assignment_but_grouping_preserves_it(self):
        tree = self.parse("val result = call(name = input, (slot = input), name = &slot)\n")
        self.assertEqual(len(list(tree.iter("named_argument_prefix"))), 2)
        self.assertEqual(len(list(tree.iter("assignment_expression"))), 1)
        self.assertEqual(len(list(tree.iter("argument_mode"))), 1)

    def test_reserved_words_are_rejected_at_the_word_and_suffix_names_survive(self):
        for word in ("async", "await", "suspend", "actor", "spawn", "sealed",
                     "dyn", "where", "yield", "macro", "reify"):
            with self.subTest(word=word):
                tree = self.parse(f"val before = 0\nval {word} = 1\nval {word}Task = 2\n", valid=False)
                errors = list(tree.iter("ERROR"))
                self.assertTrue(any(error.get("srow") == "1" and error.get("scol") == "4"
                                    and error.get("erow") == "1" and 4 < int(error.get("ecol")) <= 4 + len(word)
                                    for error in errors), ET.tostring(tree).decode())
                names = [node.text for node in tree.findall("variable_declaration/identifier[@field='name']")]
                self.assertEqual(names[0], "before")
                self.assertEqual(names[-1], f"{word}Task")
                self.assertNotIn(word, names)
                after = tree.findall("variable_declaration")[-1]
                self.assertEqual(after.get("srow"), "2")
                self.assertEqual(list(after.iter("ERROR")), [])
                self.parse(f"val {word}Task = 1\nval prefix{word} = 2\n")

    def test_context_lookahead_preserves_comments_and_token_ranges(self):
        tree = self.parse("fun f(own /* mode */ item: Int): Int = item\n"
                          "fun g(): Unit { loop /* body */ { break } }\n"
                          "val f = move /* capture */ { 1 }\n")
        mode = tree.find(".//parameter_mode")
        self.assertEqual((mode.get("scol"), mode.get("ecol")), ("6", "9"))
        self.assertEqual(len(list(tree.iter("loop_statement"))), 1)
        self.assertEqual(len(list(tree.iter("lambda_expression"))), 1)

    def test_word_operators_do_not_split_identifier_prefixes(self):
        tree = self.parse("val input = 1\nval island = 2\nval asset = 3\n"
                          "val membership = input in items\nval check = island is Int\n"
                          "val cast = asset as Int\nval safe = asset as? Int\n"
                          "val absent = input !in items\nval different = island !is Int\n")
        self.assertEqual(len(list(tree.iter("binary_expression"))), 4)
        self.assertEqual(len(list(tree.iter("cast_expression"))), 2)
        for word in ("input", "island", "asset"):
            self.parse(f"val rejected = consume(own {word})\nval after = 1\n", valid=False)

    def test_compound_word_operators_require_adjacent_whole_words(self):
        for operand in ("!input", "!island", "!inside", "!in_item", "!is2"):
            with self.subTest(operand=operand):
                tree = self.parse(f"val result = {operand}\n")
                self.assertEqual(len(list(tree.iter("prefix_expression"))), 1)
                self.assertEqual(tree.find(".//prefix_expression/expression/identifier").text, operand[1:])
                self.parse(f"val rejected = value {operand}\n", valid=False)
        for operator in ("! in", "! is", "!/*gap*/in", "!/*gap*/is", "as ?", "as/*gap*/?"):
            with self.subTest(operator=operator):
                self.parse(f"val rejected = value {operator} Target\n", valid=False)

    def test_invalid_modes_and_destructuring_wildcard_recover(self):
        for value in ("consume(own input)", "consume(borrow input)", "consume(inout input)"):
            with self.subTest(value=value):
                tree = self.parse(f"val rejected = {value}\nval after = 1\n", valid=False)
                self.assertEqual(tree.findall("variable_declaration/identifier[@field='name']")[-1].text, "after")
        self.parse("fun f(): Unit { val (first, _) = pair }\n", valid=False)
        self.parse("fun f(): Unit { val (first, second) = pair }\n")
        self.parse("fun f(): Unit { for ((first, _) in pairs) {} }\n")

    def test_explicit_block_and_ordinary_block_identifier(self):
        tree = self.parse("fun a() { block { x() } }\n")
        self.assertEqual(len(list(tree.iter("nested_block_statement"))), 1)
        self.assertEqual(list(tree.iter("lambda_expression")), [])
        ordinary = self.parse("fun a() { val block = 1\nblock(1)\nobj.block() }\n")
        self.assertEqual(list(ordinary.iter("nested_block_statement")), [])
        self.assertEqual(len(list(ordinary.iter("call_expression"))), 2)

    def test_crossline_trailing_lambda_attaches_to_parenthesized_call(self):
        tree = self.parse("fun a() { val b = f()\n{ x() } }\n")
        body = tree.find(".//function_declaration/block")
        self.assertEqual(len(list(body)), 1)
        call = body.find(".//local_variable_statement//call_expression")
        self.assertIsNotNone(call)
        self.assertEqual(len(list(call.iter("lambda_expression"))), 1)
        stmt_call = self.parse("fun a() { f()\n{ x() } }\n")
        self.assertEqual(len(list(stmt_call.findall(".//call_expression/lambda_expression"))), 1)

    def test_bare_block_statement_is_rejected_in_block_and_lambda_bodies(self):
        self.parse("fun a() { { x() } }\n", valid=False)
        self.parse("val f = { { x() } }\n", valid=False)
        grouped = self.parse("fun a() { ({ x() }) }\n")
        self.assertEqual(len(list(grouped.findall(".//grouped_expression//lambda_expression"))), 1)
        moved = self.parse("fun a() { move { x() } }\n")
        self.assertEqual(len(list(moved.iter("lambda_expression"))), 1)

    def test_if_control_block_is_not_misread_as_lambda(self):
        tree = self.parse("fun a(ok: Boolean) { if (ok) { work() } }\n")
        self.assertEqual(len(list(tree.iter("control_block"))), 1)
        self.assertEqual(list(tree.iter("lambda_expression")), [])
        loop_branch = self.parse("fun a(ok: Boolean, ready: Boolean) { if (ok) { while (ready) {} } }\n")
        self.assertEqual(len(list(loop_branch.iter("control_block"))), 1)
        self.assertEqual(len(list(loop_branch.iter("while_statement"))), 1)

    def test_same_line_unparenthesized_trailing_lambda_calls(self):
        for source in (
            "fun f() { html { render() } }\n",
            "fun f() { obj.block { render() } }\n",
            "fun f() { build<Int> { render() } }\n",
        ):
            with self.subTest(source=source):
                tree = self.parse(source)
                self.assertEqual(len(list(tree.findall(".//call_expression/lambda_expression"))), 1)

    def test_crossline_trailing_lambda_preserves_comments_and_when_entry_boundary(self):
        with_line_comment = self.parse("fun a() { val b = f() // reason\n{ work() } }\n")
        self.assertEqual(len(list(with_line_comment.findall(".//call_expression/lambda_expression"))), 1)
        self.assertEqual(len(list(with_line_comment.iter("line_comment"))), 1)
        with_block_comment = self.parse("fun a() { val b = f() /* reason */\n{ work() } }\n")
        self.assertEqual(len(list(with_block_comment.findall(".//call_expression/lambda_expression"))), 1)
        self.assertEqual(len(list(with_block_comment.iter("block_comment"))), 1)
        for source in (
            "fun a(flag: Boolean) { when { flag -> f()\n{ -> true }() -> g() } }\n",
            "fun a(flag: Boolean) { when { flag -> f()\n{ -> true }() && flag -> g() } }\n",
            "fun a(flag: Boolean) { when { flag -> f()\n{ -> true }(), flag -> g() } }\n",
            "fun a(flag: Boolean, other: Boolean) { when { flag -> f()\n{ -> true }() && !flag || other -> g() } }\n",
        ):
            with self.subTest(source=source):
                when_tree = self.parse(source)
                self.assertEqual(len(list(when_tree.iter("when_entry"))), 2)
                self.assertEqual(list(when_tree.findall(".//call_expression/lambda_expression")), [])
        attached_in_when = self.parse("fun a(flag: Boolean, other: Boolean) { when { flag -> f()\n{ work() }\nother -> g() } }\n")
        self.assertEqual(len(list(attached_in_when.iter("when_entry"))), 2)
        self.assertEqual(len(list(attached_in_when.findall(".//call_expression/lambda_expression"))), 1)

    def test_named_bitwise_operators_and_identifier_survival(self):
        tree = self.parse("val x = a shl 1 shr 2 ushr 3 and b xor c or d\n")
        self.assertEqual(len(list(tree.iter("binary_expression"))), 6)
        cont = self.parse("val bits = a shl\n  1\n")
        self.assertEqual(len(list(cont.iter("binary_expression"))), 1)
        prec = self.parse("val p = a or b and c\n")
        outer = prec.find(".//variable_declaration/expression/binary_expression")
        self.assertIsNotNone(outer)
        self.assertIn("or", "".join(outer.itertext()).split())
        right_bin = outer.find("expression[@field='right']/binary_expression")
        self.assertIsNotNone(right_bin)
        self.assertIn("and", "".join(right_bin.itertext()).split())
        self.assertIsNone(outer.find("expression[@field='left']/binary_expression"))
        for word in ("shl", "shr", "ushr", "and", "xor", "or"):
            with self.subTest(word=word):
                ident = self.parse(f"fun {word}({word}: Int): Int = {word}\nval v = {word}({word})\n")
                self.assertEqual(list(ident.iter("binary_expression")), [])

    def test_block_and_lambda_semicolons(self):
        tree = self.parse("fun f(ok: Boolean) { a(); b(); if (ok) { c(); d() }; val g = { x(); y() } }\n")
        self.assertEqual(len(list(tree.iter("call_expression"))), 6)
        self.assertEqual(len(list(tree.iter("control_block"))), 1)
        self.assertEqual(len(list(tree.iter("lambda_expression"))), 1)
        for valid in (
            "fun f() { }\n",
            "fun f() { ; }\n",
            "fun f() { 1; 2 }\n",
            "fun f() { 1\n2 }\n",
            "val g = { -> }\n",
            "val g = { x ->\n x(); x() }\n",
        ):
            with self.subTest(valid=valid):
                self.parse(valid)
        for invalid in (
            "fun f() { 1 2 }\n",
            "fun f() { a() b() }\n",
            "fun f() { val x = 1 val y = 2 }\n",
            "fun f(ok: Boolean) { if (ok) { a() b() } }\n",
            "val g = { x() y() }\n",
            "fun f() { run { a() b() } }\n",
        ):
            with self.subTest(invalid=invalid):
                self.parse(invalid, valid=False)

    def test_crossline_move_lambda_and_function_type(self):
        tree = self.parse(
            "val f = move\n{ 1 }\n"
            "val f2 = move\n// comment\n{ 1 }\n"
            "val g: move\n(own Int) -> Int = move // cap\n{ n -> n }\n"
        )
        self.assertEqual(len(list(tree.iter("lambda_expression"))), 3)
        div = self.parse("val d = move / { 2 }()\n")
        self.assertEqual(len(list(div.iter("binary_expression"))), 1)
        self.assertEqual(len(list(div.iter("lambda_expression"))), 1)
        left_id = div.find(".//binary_expression/expression[@field='left']/identifier")
        self.assertIsNotNone(left_id)
        self.assertEqual(left_id.text, "move")

    def test_bare_cr_and_form_feed_rejected_while_crlf_accepted(self):
        for invalid in ("val x =\r 1\n", "val x =\f 1\n", "// comment\rval x = 1\n"):
            with self.subTest(invalid=invalid):
                self.parse(invalid, valid=False)
        crlf = self.parse("val x = 1\r\nval y = 2\r\n")
        self.assertEqual(len(list(crlf.iter("variable_declaration"))), 2)
        comment_crlf = self.parse("// comment\r\nval x = 1\n")
        self.assertEqual(len(list(comment_crlf.iter("variable_declaration"))), 1)

    def test_multiline_block_comment_splits_statements_while_inline_stays_horizontal(self):
        for multiline in ("fun f() {\n    a/*\n*/-b\n}\n", "fun f() {\n    a/*\r\n*/-b\n}\n"):
            with self.subTest(multiline=multiline):
                tree = self.parse(multiline)
                self.assertEqual(len(list(tree.iter("prefix_expression"))), 1)
                self.assertEqual(list(tree.iter("binary_expression")), [])
                self.assertEqual(len(list(tree.iter("block_comment"))), 1)
        inline = self.parse("fun f() {\n    a/* inline */-b\n}\n")
        self.assertEqual(list(inline.iter("prefix_expression")), [])
        self.assertEqual(len(list(inline.iter("binary_expression"))), 1)
        self.assertEqual(len(list(inline.iter("block_comment"))), 1)
        # Operator line continuation across a multiline block comment (Guide 04/06)
        cont = self.parse("val x = a +/*\n*/b\n")
        self.assertEqual(len(list(cont.iter("binary_expression"))), 1)
        self.assertEqual(len(list(cont.iter("block_comment"))), 1)
        # Cross-line trailing lambda across a multiline block comment (Guide 07)
        trailing = self.parse("val x = f()/*\n*/{ 1 }\n")
        self.assertEqual(len(list(trailing.findall(".//call_expression/lambda_expression"))), 1)

    def test_non_associative_operator_groups_reject_unparenthesized_chains(self):
        for invalid in (
            "val x = a..b..c\n",
            "val x = a..b..<c\n",
            "val x = a..<b..c\n",
            "val x = a in b in c\n",
            "val x = a in b !in c\n",
            "val x = a in b is Int\n",
            "val x = a is Int is Any\n",
            "val x = a < b <= c\n",
            "val x = a < b < c\n",
            "val x = a >= b > c\n",
            "val x = a == b != c\n",
            "val x = a == b == c\n",
        ):
            with self.subTest(invalid=invalid):
                self.parse(invalid, valid=False)
        for valid in (
            "val x = (a < b) < c\n",
            "val x = (a == b) != c\n",
            "val x = (a..b)..<c\n",
            "val x = (a in b) in c\n",
            "val x = a in 1..10\n",
            "val x = a < b == true\n",
            "val x = a in items == true\n",
            "val x = a is Int == true\n",
            "val x = a < b && b < c\n",
            "val x = f<Int>(a)\n",
        ):
            with self.subTest(valid=valid):
                self.parse(valid)
        elvis_to = self.parse("val x = a ?: b to c\n")
        outer = elvis_to.find(".//variable_declaration/expression/binary_expression")
        self.assertIsNotNone(outer)
        self.assertIsNotNone(outer.find("expression[@field='right']/binary_expression/to_operator"))


if __name__ == "__main__":
    unittest.main()
