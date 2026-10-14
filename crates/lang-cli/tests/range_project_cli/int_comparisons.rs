//! SPEC-0315: migrate the four old CLI rejections to actual project executions.
use super::*;

#[derive(Clone, Copy, Debug)]
enum Position {
    Before,
    Live,
    After,
}

fn comparisons(operator: &str, limit: i32, outcomes: [bool; 6]) {
    for constant in [false, true] {
        let declaration = if constant {
            format!("const val LIMIT:Int={limit}")
        } else {
            String::new()
        };
        let operand = if constant {
            "LIMIT".to_owned()
        } else {
            limit.to_string()
        };
        let expression = format!("n {operator} {operand}");
        let comparison = format!("val other={expression};show(other)");
        for position in [Position::Before, Position::Live, Position::After] {
            let body = match position {
                Position::Before => {
                    format!("{comparison};{{borrow val parent=take(root,1);read(parent)}}")
                }
                Position::Live => format!(
                    "{{borrow val parent=take(root,1);read(parent);{comparison};read(parent)}}"
                ),
                Position::After => {
                    format!("{{borrow val parent=take(root,1);read(parent)}};{comparison}")
                }
            };
            let project = Project::new();
            // Only the project loader installs/authorizes the real range std body.
            // A const declaration selects the CLI's validated constant unit path;
            // the literal fixture uses its basic unit path.
            let source = format!(
                r#"package app
import koven.algorithms.take
{declaration}
fun show(flag:Boolean):Unit{{if(flag){{println("true")}}else{{println("false")}}}}
fun read(view:View<String>):Unit{{for(item in view){{println(item)}}}}
fun consume(own root:List<String>):Unit{{println("consume")}}
fun scan(n:Int):Unit{{val root=listOf("kept","excluded");{body};consume(root)}}
fun main():Unit{{scan(-2147483647-1);scan(-1);scan(2);scan(3);scan(4);scan(2147483647)}}
"#
            );
            let built = project.build(&source);
            assert!(
                built.status.success(),
                "{expression}/{position:?}: {built:?}"
            );
            assert!(built.stdout.is_empty(), "{built:?}");
            assert!(built.stderr.is_empty(), "{built:?}");
            let expected = outcomes
                .into_iter()
                .map(|outcome| {
                    let value = if outcome { "true" } else { "false" };
                    match position {
                        Position::Before => format!("{value}\nkept\nconsume\n"),
                        Position::Live => format!("kept\n{value}\nkept\nconsume\n"),
                        Position::After => format!("kept\n{value}\nconsume\n"),
                    }
                })
                .collect::<String>();
            let run = Command::new(project.0.join("program")).output().unwrap();
            assert_eq!(run.status.code(), Some(0), "{run:?}");
            assert_eq!(run.stdout, expected.as_bytes(), "{expression}/{position:?}");
            assert!(run.stderr.is_empty(), "{run:?}");
        }
    }
}

#[test]
fn public_std_range_int_not_equal_at_all_binding_positions() {
    comparisons("!=", 2, [true, true, false, true, true, true]);
}

#[test]
fn public_std_range_int_less_equal_at_all_binding_positions() {
    comparisons("<=", 3, [true, true, true, true, false, false]);
}

#[test]
fn public_std_range_int_greater_at_all_binding_positions() {
    comparisons(">", 2, [false, false, false, true, true, true]);
}

#[test]
fn public_std_range_int_greater_equal_at_all_binding_positions() {
    comparisons(">=", 3, [false, false, false, true, true, true]);
}
