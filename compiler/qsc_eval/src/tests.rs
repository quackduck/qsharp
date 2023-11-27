// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::{
    backend::{Backend, SparseSim},
    debug::{map_hir_package_to_fir, Frame},
    output::{GenericReceiver, Receiver},
    val::{self, GlobalId},
    Env, Error, Global, NodeLookup, State, StepAction, StepResult, Value,
};
use expect_test::{expect, Expect};
use indoc::indoc;
use qsc_data_structures::index_map::IndexMap;

use qsc_fir::fir::{BlockId, ExprId, ItemKind, PackageId, PatId, StmtId};
use qsc_frontend::compile::{self, compile, PackageStore, SourceMap, TargetProfile};

use qsc_passes::{run_core_passes, run_default_passes, PackageType};
/// Evaluates the given expression with the given context.
/// Creates a new environment and simulator.
/// # Errors
/// Returns the first error encountered during execution.
pub(super) fn eval_expr(
    expr: ExprId,
    sim: &mut impl Backend<ResultType = impl Into<val::Result>>,
    globals: &impl NodeLookup,
    package: PackageId,
    out: &mut impl Receiver,
) -> Result<Value, (Error, Vec<Frame>)> {
    let mut state = State::new(package);
    let mut env = Env::with_empty_scope();
    state.push_expr(expr);
    let StepResult::Return(value) =
        state.eval(globals, &mut env, sim, out, &[], StepAction::Continue)?
    else {
        unreachable!("eval_expr should always return a value");
    };
    Ok(value)
}

struct Lookup<'a> {
    fir_store: &'a IndexMap<PackageId, qsc_fir::fir::Package>,
}

impl<'a> Lookup<'a> {
    fn get_package(&self, package: PackageId) -> &qsc_fir::fir::Package {
        self.fir_store
            .get(package)
            .expect("Package should be in FIR store")
    }
}

impl<'a> NodeLookup for Lookup<'a> {
    fn get(&self, id: GlobalId) -> Option<Global<'a>> {
        get_global(self.fir_store, id)
    }
    fn get_block(&self, package: PackageId, id: BlockId) -> &qsc_fir::fir::Block {
        self.get_package(package)
            .blocks
            .get(id)
            .expect("BlockId should have been lowered")
    }
    fn get_expr(&self, package: PackageId, id: ExprId) -> &qsc_fir::fir::Expr {
        self.get_package(package)
            .exprs
            .get(id)
            .expect("ExprId should have been lowered")
    }
    fn get_pat(&self, package: PackageId, id: PatId) -> &qsc_fir::fir::Pat {
        self.get_package(package)
            .pats
            .get(id)
            .expect("PatId should have been lowered")
    }
    fn get_stmt(&self, package: PackageId, id: StmtId) -> &qsc_fir::fir::Stmt {
        self.get_package(package)
            .stmts
            .get(id)
            .expect("StmtId should have been lowered")
    }
}

pub(super) fn get_global(
    fir_store: &IndexMap<PackageId, qsc_fir::fir::Package>,
    id: GlobalId,
) -> Option<Global> {
    fir_store
        .get(id.package)
        .and_then(|package| match &package.items.get(id.item)?.kind {
            ItemKind::Callable(callable) => Some(Global::Callable(callable)),
            ItemKind::Namespace(..) => None,
            ItemKind::Ty(..) => Some(Global::Udt),
        })
}

fn check_expr(file: &str, expr: &str, expect: &Expect) {
    let mut fir_lowerer = crate::lower::Lowerer::new();
    let mut core = compile::core();
    run_core_passes(&mut core);
    let core_fir = fir_lowerer.lower_package(&core.package);
    let mut store = PackageStore::new(core);

    let mut std = compile::std(&store, TargetProfile::Full);
    assert!(std.errors.is_empty());
    assert!(run_default_passes(
        store.core(),
        &mut std,
        PackageType::Lib,
        TargetProfile::Full
    )
    .is_empty());
    let std_fir = fir_lowerer.lower_package(&std.package);
    let std_id = store.insert(std);

    let sources = SourceMap::new([("test".into(), file.into())], Some(expr.into()));
    let mut unit = compile(&store, &[std_id], sources, TargetProfile::Full);
    assert!(unit.errors.is_empty(), "{:?}", unit.errors);
    let pass_errors = run_default_passes(
        store.core(),
        &mut unit,
        PackageType::Lib,
        TargetProfile::Full,
    );
    assert!(pass_errors.is_empty(), "{pass_errors:?}");
    let unit_fir = fir_lowerer.lower_package(&unit.package);
    let entry = unit_fir.entry.expect("package should have entry");
    let id = store.insert(unit);

    let mut fir_store = IndexMap::new();
    fir_store.insert(
        map_hir_package_to_fir(qsc_hir::hir::PackageId::CORE),
        core_fir,
    );
    fir_store.insert(map_hir_package_to_fir(std_id), std_fir);
    fir_store.insert(map_hir_package_to_fir(id), unit_fir);

    let mut out = Vec::new();
    let lookup = Lookup {
        fir_store: &fir_store,
    };
    match eval_expr(
        entry,
        &mut SparseSim::new(),
        &lookup,
        map_hir_package_to_fir(id),
        &mut GenericReceiver::new(&mut out),
    ) {
        Ok(value) => expect.assert_eq(&value.to_string()),
        Err(err) => expect.assert_debug_eq(&err),
    }
}

#[test]
fn array_expr() {
    check_expr("", "[1, 2, 3]", &expect!["[1, 2, 3]"]);
}

#[test]
fn array_repeat_expr() {
    check_expr("", "[4, size = 3]", &expect!["[4, 4, 4]"]);
}

#[test]
fn block_expr() {
    check_expr(
        "",
        indoc! { "{
            let x = 1;
            let y = x;
            y
        }"},
        &expect!["1"],
    );
}

#[test]
fn block_empty_is_unit_expr() {
    check_expr("", "{}", &expect!["()"]);
}

#[test]
fn block_shadowing_expr() {
    check_expr(
        "",
        indoc! { "{
            let x = 1;
            let x = 2;
            x
        }"},
        &expect!["2"],
    );
}

#[test]
fn block_nested_shadowing_expr() {
    check_expr(
        "",
        indoc! { "{
            let x = 1;
            let y = {
                let x = 2;
                x
            };
            (y, x)
        }"},
        &expect!["(2, 1)"],
    );
}

#[test]
fn block_let_bind_tuple_expr() {
    check_expr(
        "",
        indoc! {"{
            let x = (1, 2);
            let (y, z) = x;
            (z, y)
        }"},
        &expect!["(2, 1)"],
    );
}

#[test]
fn block_mutable_expr() {
    check_expr(
        "",
        indoc! {"{
            mutable x = 0;
            x
        }"},
        &expect!["0"],
    );
}

#[test]
fn block_mutable_update_expr() {
    check_expr(
        "",
        indoc! {"{
            mutable x = 0;
            set x = 1;
            x
        }"},
        &expect!["1"],
    );
}

#[test]
fn block_mutable_update_tuple_expr() {
    check_expr(
        "",
        indoc! {"{
            mutable x = (0, 1);
            set x = (1, 2);
            x
        }"},
        &expect!["(1, 2)"],
    );
}

#[test]
fn block_mutable_update_tuple_item_expr() {
    check_expr(
        "",
        indoc! {"{
            mutable (x, y) = (0, 1);
            set (x, y) = (1, 2);
            (x, y)
        }"},
        &expect!["(1, 2)"],
    );
}

#[test]
fn block_mutable_update_tuple_hole_expr() {
    check_expr(
        "",
        indoc! {"{
            mutable (x, y) = (0, 1);
            set (_, y) = (1, 2);
            (x, y)
        }"},
        &expect!["(0, 2)"],
    );
}

#[test]
fn block_mutable_nested_scopes_expr() {
    check_expr(
        "",
        indoc! {"{
            mutable x = 0;
            {
                mutable y = 1;
                set x = y;
            }
            x
        }"},
        &expect!["1"],
    );
}

#[test]
fn block_mutable_nested_scopes_shadowing_expr() {
    check_expr(
        "",
        indoc! {"{
            mutable x = 0;
            {
                mutable x = 1;
                set x = 2;
            }
            x
        }"},
        &expect!["0"],
    );
}

#[test]
fn block_qubit_use_expr() {
    check_expr(
        "",
        indoc! {"{
            use q = Qubit();
            q
        }"},
        &expect!["Qubit0"],
    );
}

#[test]
fn block_qubit_use_use_expr() {
    check_expr(
        "",
        indoc! {"{
            use q = Qubit();
            use q1 = Qubit();
            q1
        }"},
        &expect!["Qubit1"],
    );
}

#[test]
fn block_qubit_use_reuse_expr() {
    check_expr(
        "",
        indoc! {"{
            {
                use q = Qubit();
            }
            use q = Qubit();
            q
        }"},
        &expect!["Qubit0"],
    );
}

#[test]
fn block_qubit_use_scope_reuse_expr() {
    check_expr(
        "",
        indoc! {"{
            use q = Qubit() {
            }
            use q = Qubit();
            q
        }"},
        &expect!["Qubit0"],
    );
}

#[test]
fn block_qubit_use_array_expr() {
    check_expr(
        "",
        indoc! {"{
            use q = Qubit[3];
            q
        }"},
        &expect!["[Qubit0, Qubit1, Qubit2]"],
    );
}

#[test]
fn block_qubit_use_array_invalid_count_expr() {
    check_expr(
        "",
        indoc! {"{
            use q = Qubit[-3];
            q
        }"},
        &expect![[r#"
            (
                UserFail(
                    "Cannot allocate qubit array with a negative length",
                    PackageSpan {
                        package: PackageId(
                            0,
                        ),
                        span: Span {
                            lo: 1568,
                            hi: 1625,
                        },
                    },
                ),
                [
                    Frame {
                        span: Span {
                            lo: 1573,
                            hi: 1625,
                        },
                        id: GlobalId {
                            package: PackageId(
                                0,
                            ),
                            item: LocalItemId(
                                6,
                            ),
                        },
                        caller: PackageId(
                            2,
                        ),
                        functor: FunctorApp {
                            adjoint: false,
                            controlled: 0,
                        },
                    },
                ],
            )
        "#]],
    );
}

#[test]
fn block_qubit_use_tuple_expr() {
    check_expr(
        "",
        indoc! {"{
            use q = (Qubit[3], Qubit(), Qubit());
            q
        }"},
        &expect!["([Qubit0, Qubit1, Qubit2], Qubit3, Qubit4)"],
    );
}

#[test]
fn block_qubit_use_nested_tuple_expr() {
    check_expr(
        "",
        indoc! {"{
            use q = (Qubit[3], (Qubit(), Qubit()));
            q
        }"},
        &expect!["([Qubit0, Qubit1, Qubit2], (Qubit3, Qubit4))"],
    );
}

#[test]
fn binop_add_array() {
    check_expr("", "[1, 2] + [3, 4]", &expect!["[1, 2, 3, 4]"]);
}

#[test]
fn binop_add_bigint() {
    check_expr(
        "",
        "2L + 9_223_372_036_854_775_808L",
        &expect!["9223372036854775810"],
    );
}

#[test]
fn binop_add_double() {
    check_expr("", "2.8 + 5.4", &expect!["8.2"]);
}

#[test]
fn binop_add_int() {
    check_expr("", "28 + 54", &expect!["82"]);
}

#[test]
fn binop_add_int_wrap() {
    check_expr(
        "",
        "0x7FFFFFFFFFFFFFFF + 1",
        &expect!["-9223372036854775808"],
    );
}

#[test]
fn binop_add_string() {
    check_expr("", r#""Hello," + " World!""#, &expect!["Hello, World!"]);
}

#[test]
fn binop_andb_bigint() {
    check_expr("", "28L &&& 54L", &expect!["20"]);
}

#[test]
fn binop_andb_int() {
    check_expr("", "28 &&& 54", &expect!["20"]);
}

#[test]
fn binop_andl() {
    check_expr("", "true and true", &expect!["true"]);
}

#[test]
fn binop_andl_false() {
    check_expr("", "true and false", &expect!["false"]);
}

#[test]
fn binop_andl_no_shortcut() {
    check_expr(
        "",
        r#"true and (fail "Should Fail")"#,
        &expect![[r#"
            (
                UserFail(
                    "Should Fail",
                    PackageSpan {
                        package: PackageId(
                            2,
                        ),
                        span: Span {
                            lo: 10,
                            hi: 28,
                        },
                    },
                ),
                [],
            )
        "#]],
    );
}

#[test]
fn binop_div_bigint() {
    check_expr("", "12L / 3L", &expect!["4"]);
}

#[test]
fn binop_div_bigint_zero() {
    check_expr(
        "",
        "12L / 0L",
        &expect![[r#"
            (
                DivZero(
                    PackageSpan {
                        package: PackageId(
                            2,
                        ),
                        span: Span {
                            lo: 6,
                            hi: 8,
                        },
                    },
                ),
                [],
            )
        "#]],
    );
}

#[test]
fn binop_div_int() {
    check_expr("", "12 / 3", &expect!["4"]);
}

#[test]
fn binop_div_int_wrap() {
    check_expr(
        "",
        "(-0x8000000000000000) / (-1)",
        &expect!["-9223372036854775808"],
    );
}

#[test]
fn binop_div_int_zero() {
    check_expr(
        "",
        "12 / 0",
        &expect![[r#"
            (
                DivZero(
                    PackageSpan {
                        package: PackageId(
                            2,
                        ),
                        span: Span {
                            lo: 5,
                            hi: 6,
                        },
                    },
                ),
                [],
            )
        "#]],
    );
}

#[test]
fn binop_div_double() {
    check_expr("", "1.2 / 0.3", &expect!["4.0"]);
}

#[test]
fn binop_div_double_zero() {
    check_expr(
        "",
        "1.2 / 0.0",
        &expect![[r#"
            (
                DivZero(
                    PackageSpan {
                        package: PackageId(
                            2,
                        ),
                        span: Span {
                            lo: 6,
                            hi: 9,
                        },
                    },
                ),
                [],
            )
        "#]],
    );
}

#[test]
fn binop_eq_double() {
    check_expr("", "1.2 / 0.3", &expect!["4.0"]);
}

#[test]
fn binop_equal_array() {
    check_expr("", "[1, 2, 3] == [1, 2, 3]", &expect!["true"]);
}

#[test]
fn binop_equal_array_false_content() {
    check_expr("", "[1, 2, 3] == [1, 0, 3]", &expect!["false"]);
}

#[test]
fn binop_equal_array_false_length() {
    check_expr("", "[1, 2, 3] == [1, 2, 3, 4]", &expect!["false"]);
}

#[test]
fn binop_equal_bigint() {
    check_expr("", "18L == 18L", &expect!["true"]);
}

#[test]
fn binop_equal_bigint_false() {
    check_expr("", "18L == 8L", &expect!["false"]);
}

#[test]
fn binop_equal_bool() {
    check_expr("", "false == false", &expect!["true"]);
}

#[test]
fn binop_equal_bool_false() {
    check_expr("", "false == true", &expect!["false"]);
}

#[test]
fn binop_equal_double() {
    check_expr("", "1.254 == 1.254", &expect!["true"]);
}

#[test]
fn binop_equal_double_false() {
    check_expr("", "1.254 == 1.25", &expect!["false"]);
}

#[test]
fn binop_equal_int() {
    check_expr("", "42 == 42", &expect!["true"]);
}

#[test]
fn binop_equal_int_false() {
    check_expr("", "42 == 43", &expect!["false"]);
}

#[test]
fn binop_equal_pauli() {
    check_expr("", "PauliX == PauliX", &expect!["true"]);
}

#[test]
fn binop_equal_pauli_false() {
    check_expr("", "PauliX == PauliZ", &expect!["false"]);
}

#[test]
fn binop_equal_range() {
    check_expr("", "(0..4) == (0..4)", &expect!["true"]);
}

#[test]
fn binop_equal_range_false() {
    check_expr("", "(0..2..4) == (0..4)", &expect!["false"]);
}

#[test]
fn binop_equal_result() {
    check_expr("", "One == One", &expect!["true"]);
}

#[test]
fn binop_equal_result_false() {
    check_expr("", "One == Zero", &expect!["false"]);
}

#[test]
fn binop_equal_string() {
    check_expr("", r#""foo" == "foo""#, &expect!["true"]);
}

#[test]
fn binop_equal_string_false() {
    check_expr("", r#""foo" == "bar""#, &expect!["false"]);
}

#[test]
fn binop_equal_tuple() {
    check_expr("", "(1, 2, 3) == (1, 2, 3)", &expect!["true"]);
}

#[test]
fn binop_equal_tuple_false_content() {
    check_expr("", "(1, 2, 3) == (1, -2, 3)", &expect!["false"]);
}

#[test]
fn binop_exp_bigint() {
    check_expr("", "2L^3", &expect!["8"]);
}

#[test]
fn binop_exp_bigint_zero_exp() {
    check_expr("", "2L^0", &expect!["1"]);
}

#[test]
fn binop_exp_bigint_neg_zero_exp() {
    check_expr("", "(-2L)^0", &expect!["1"]);
}

#[test]
fn binop_exp_bigint_negative_exp() {
    check_expr(
        "",
        "2L^-3",
        &expect![[r#"
            (
                InvalidNegativeInt(
                    -3,
                    PackageSpan {
                        package: PackageId(
                            2,
                        ),
                        span: Span {
                            lo: 3,
                            hi: 5,
                        },
                    },
                ),
                [],
            )
        "#]],
    );
}

#[test]
fn binop_exp_bigint_too_large() {
    check_expr(
        "",
        "2L^9_223_372_036_854_775_807",
        &expect![[r#"
            (
                IntTooLarge(
                    9223372036854775807,
                    PackageSpan {
                        package: PackageId(
                            2,
                        ),
                        span: Span {
                            lo: 3,
                            hi: 28,
                        },
                    },
                ),
                [],
            )
        "#]],
    );
}

#[test]
fn binop_exp_double() {
    check_expr("", "2.3^3.1", &expect!["13.22380059125472"]);
}

#[test]
fn binop_exp_double_negative_exp() {
    check_expr("", "2.3^-3.1", &expect!["0.07562122501010253"]);
}

#[test]
fn binop_exp_int() {
    check_expr("", "2^3", &expect!["8"]);
}

#[test]
fn binop_exp_int_zero_exp() {
    check_expr("", "2^0", &expect!["1"]);
}

#[test]
fn binop_exp_int_neg_zero_exp() {
    check_expr("", "(-2)^0", &expect!["1"]);
}

#[test]
fn binop_exp_int_negative_exp() {
    check_expr(
        "",
        "2^-3",
        &expect![[r#"
            (
                InvalidNegativeInt(
                    -3,
                    PackageSpan {
                        package: PackageId(
                            2,
                        ),
                        span: Span {
                            lo: 2,
                            hi: 4,
                        },
                    },
                ),
                [],
            )
        "#]],
    );
}

#[test]
fn binop_exp_int_too_large() {
    check_expr(
        "",
        "100^50",
        &expect![[r#"
            (
                IntTooLarge(
                    50,
                    PackageSpan {
                        package: PackageId(
                            2,
                        ),
                        span: Span {
                            lo: 4,
                            hi: 6,
                        },
                    },
                ),
                [],
            )
        "#]],
    );
}

#[test]
fn binop_gt_bigint() {
    check_expr("", "23L > 3L", &expect!["true"]);
}

#[test]
fn binop_gt_bigint_false() {
    check_expr("", "2L > 3L", &expect!["false"]);
}

#[test]
fn binop_gt_int() {
    check_expr("", "23 > 3", &expect!["true"]);
}

#[test]
fn binop_gt_int_false() {
    check_expr("", "2 > 3", &expect!["false"]);
}

#[test]
fn binop_gt_double() {
    check_expr("", "2.3 > 0.3", &expect!["true"]);
}

#[test]
fn binop_gt_double_false() {
    check_expr("", "0.2 > 0.3", &expect!["false"]);
}

#[test]
fn binop_gte_bigint() {
    check_expr("", "23L >= 3L", &expect!["true"]);
}

#[test]
fn binop_gte_bigint_false() {
    check_expr("", "2L >= 3L", &expect!["false"]);
}

#[test]
fn binop_gte_bigint_eq() {
    check_expr("", "3L >= 3L", &expect!["true"]);
}

#[test]
fn binop_gte_int() {
    check_expr("", "23 >= 3", &expect!["true"]);
}

#[test]
fn binop_gte_int_false() {
    check_expr("", "2 >= 3", &expect!["false"]);
}

#[test]
fn binop_gte_int_eq() {
    check_expr("", "3 >= 3", &expect!["true"]);
}

#[test]
fn binop_gte_double() {
    check_expr("", "2.3 >= 0.3", &expect!["true"]);
}

#[test]
fn binop_gte_double_false() {
    check_expr("", "0.2 >= 0.3", &expect!["false"]);
}

#[test]
fn binop_gte_double_eq() {
    check_expr("", "0.3 >= 0.3", &expect!["true"]);
}

#[test]
fn binop_lt_bigint_false() {
    check_expr("", "23L < 3L", &expect!["false"]);
}

#[test]
fn binop_lt_bigint() {
    check_expr("", "2L < 3L", &expect!["true"]);
}

#[test]
fn binop_lt_int_false() {
    check_expr("", "23 < 3", &expect!["false"]);
}

#[test]
fn binop_lt_int() {
    check_expr("", "2 < 3", &expect!["true"]);
}

#[test]
fn binop_lt_double_false() {
    check_expr("", "2.3 < 0.3", &expect!["false"]);
}

#[test]
fn binop_lt_double() {
    check_expr("", "0.2 < 0.3", &expect!["true"]);
}

#[test]
fn binop_lte_bigint_false() {
    check_expr("", "23L <= 3L", &expect!["false"]);
}

#[test]
fn binop_lte_bigint() {
    check_expr("", "2L <= 3L", &expect!["true"]);
}

#[test]
fn binop_lte_bigint_eq() {
    check_expr("", "3L <= 3L", &expect!["true"]);
}

#[test]
fn binop_lte_int_false() {
    check_expr("", "23 <= 3", &expect!["false"]);
}

#[test]
fn binop_lte_int() {
    check_expr("", "2 <= 3", &expect!["true"]);
}

#[test]
fn binop_lte_int_eq() {
    check_expr("", "3 <= 3", &expect!["true"]);
}

#[test]
fn binop_lte_double_false() {
    check_expr("", "2.3 <= 0.3", &expect!["false"]);
}

#[test]
fn binop_lte_double() {
    check_expr("", "0.2 <= 0.3", &expect!["true"]);
}

#[test]
fn binop_lte_double_eq() {
    check_expr("", "0.3 <= 0.3", &expect!["true"]);
}

#[test]
fn binop_mod_bigint() {
    check_expr("", "8L % 6L", &expect!["2"]);
}

#[test]
fn binop_mod_bigint_zero() {
    check_expr(
        "",
        "12L % 0L",
        &expect![[r#"
            (
                DivZero(
                    PackageSpan {
                        package: PackageId(
                            2,
                        ),
                        span: Span {
                            lo: 6,
                            hi: 8,
                        },
                    },
                ),
                [],
            )
        "#]],
    );
}

#[test]
fn binop_mod_int() {
    check_expr("", "8 % 6", &expect!["2"]);
}

#[test]
fn binop_mod_int_wrap() {
    check_expr("", "(-0x8000000000000000) % (-1)", &expect!["0"]);
}

#[test]
fn binop_mod_int_zero() {
    check_expr(
        "",
        "12 % 0",
        &expect![[r#"
            (
                DivZero(
                    PackageSpan {
                        package: PackageId(
                            2,
                        ),
                        span: Span {
                            lo: 5,
                            hi: 6,
                        },
                    },
                ),
                [],
            )
        "#]],
    );
}

#[test]
fn binop_mod_double() {
    check_expr("", "8.411 % 6.833", &expect!["1.5779999999999994"]);
}

#[test]
fn binop_mod_double_zero() {
    check_expr(
        "",
        "1.2 % 0.0",
        &expect![[r#"
            (
                DivZero(
                    PackageSpan {
                        package: PackageId(
                            2,
                        ),
                        span: Span {
                            lo: 6,
                            hi: 9,
                        },
                    },
                ),
                [],
            )
        "#]],
    );
}

#[test]
fn binop_mul_bigint() {
    check_expr("", "8L * 6L", &expect!["48"]);
}

#[test]
fn binop_mul_int() {
    check_expr("", "8 * 6", &expect!["48"]);
}
#[test]
fn binop_mul_int_wrap() {
    check_expr(
        "",
        "0x7FFFFFFFFFFFFFFF * 0xFF",
        &expect!["9223372036854775553"],
    );
}

#[test]
fn binop_mul_double() {
    check_expr("", "8.411 * 6.833", &expect!["57.472363"]);
}

#[test]
fn binop_neq_array() {
    check_expr("", "[1, 2, 3] != [1, 2, 3]", &expect!["false"]);
}

#[test]
fn binop_neq_array_true_content() {
    check_expr("", "[1, 2, 3] != [1, 0, 3]", &expect!["true"]);
}

#[test]
fn binop_neq_array_true_length() {
    check_expr("", "[1, 2, 3] != [1, 2, 3, 4]", &expect!["true"]);
}

#[test]
fn binop_neq_bigint() {
    check_expr("", "18L != 18L", &expect!["false"]);
}

#[test]
fn binop_neq_bigint_true() {
    check_expr("", "18L != 8L", &expect!["true"]);
}

#[test]
fn binop_neq_bool() {
    check_expr("", "false != false", &expect!["false"]);
}

#[test]
fn binop_neq_bool_true() {
    check_expr("", "false != true", &expect!["true"]);
}

#[test]
fn binop_neq_double() {
    check_expr("", "1.254 != 1.254", &expect!["false"]);
}

#[test]
fn binop_neq_double_true() {
    check_expr("", "1.254 != 1.25", &expect!["true"]);
}

#[test]
fn binop_neq_int() {
    check_expr("", "42 != 42", &expect!["false"]);
}

#[test]
fn binop_neq_int_true() {
    check_expr("", "42 != 43", &expect!["true"]);
}

#[test]
fn binop_neq_pauli() {
    check_expr("", "PauliX != PauliX", &expect!["false"]);
}

#[test]
fn binop_neq_pauli_true() {
    check_expr("", "PauliX != PauliZ", &expect!["true"]);
}

#[test]
fn binop_neq_range() {
    check_expr("", "(0..4) != (0..4)", &expect!["false"]);
}

#[test]
fn binop_neq_range_true() {
    check_expr("", "(0..2..4) != (0..4)", &expect!["true"]);
}

#[test]
fn binop_neq_result() {
    check_expr("", "One != One", &expect!["false"]);
}

#[test]
fn binop_neq_result_true() {
    check_expr("", "One != Zero", &expect!["true"]);
}

#[test]
fn binop_neq_string() {
    check_expr("", r#""foo" != "foo""#, &expect!["false"]);
}

#[test]
fn binop_neq_string_true() {
    check_expr("", r#""foo" != "bar""#, &expect!["true"]);
}

#[test]
fn binop_neq_tuple() {
    check_expr("", "(1, 2, 3) != (1, 2, 3)", &expect!["false"]);
}

#[test]
fn binop_neq_tuple_true_content() {
    check_expr("", "(1, 2, 3) != (1, -2, 3)", &expect!["true"]);
}

#[test]
fn binop_orb_bigint() {
    check_expr("", "28L ||| 54L", &expect!["62"]);
}

#[test]
fn binop_orb_int() {
    check_expr("", "28 ||| 54", &expect!["62"]);
}

#[test]
fn binop_orl() {
    check_expr("", "true or true", &expect!["true"]);
}

#[test]
fn binop_orl_true_lhs() {
    check_expr("", "true or false", &expect!["true"]);
}

#[test]
fn binop_orl_true_rhs() {
    check_expr("", "false or true", &expect!["true"]);
}

#[test]
fn binop_orl_false() {
    check_expr("", "false or false", &expect!["false"]);
}

#[test]
fn binop_orl_shortcut() {
    check_expr("", r#"true or (fail "Shouldn't Fail")"#, &expect!["true"]);
}

#[test]
fn binop_shl_bigint() {
    check_expr("", "4L <<< 2", &expect!["16"]);
}

#[test]
fn binop_shl_bigint_negative() {
    check_expr("", "4L <<< -2", &expect!["1"]);
}

#[test]
fn binop_shl_int() {
    check_expr("", "4 <<< 2", &expect!["16"]);
}

#[test]
fn binop_shl_int_negative() {
    check_expr("", "4 <<< -2", &expect!["1"]);
}

#[test]
fn binop_shl_int_truncate() {
    check_expr("", "1 <<< 63", &expect!["-9223372036854775808"]);
    check_expr("", "2 <<< 63", &expect!["0"]);
}

#[test]
fn binop_shl_int_overflow() {
    check_expr(
        "",
        "1 <<< 64",
        &expect![[r#"
            (
                IntTooLarge(
                    64,
                    PackageSpan {
                        package: PackageId(
                            2,
                        ),
                        span: Span {
                            lo: 6,
                            hi: 8,
                        },
                    },
                ),
                [],
            )
        "#]],
    );
}

#[test]
fn binop_shr_bigint() {
    check_expr("", "4L >>> 2", &expect!["1"]);
}

#[test]
fn binop_shr_bigint_negative() {
    check_expr("", "4L >>> -2", &expect!["16"]);
}

#[test]
fn binop_shr_int() {
    check_expr("", "4 >>> 2", &expect!["1"]);
}

#[test]
fn binop_shr_int_negative() {
    check_expr("", "4 >>> -2", &expect!["16"]);
}

#[test]
fn binop_shr_int_truncate() {
    check_expr("", "(-9223372036854775808) >>> 63", &expect!["-1"]);
    check_expr("", "1 >>> 63", &expect!["0"]);
}

#[test]
fn binop_shr_int_overflow() {
    check_expr(
        "",
        "1 >>> 64",
        &expect![[r#"
            (
                IntTooLarge(
                    64,
                    PackageSpan {
                        package: PackageId(
                            2,
                        ),
                        span: Span {
                            lo: 6,
                            hi: 8,
                        },
                    },
                ),
                [],
            )
        "#]],
    );
}

#[test]
fn binop_sub_bigint() {
    check_expr("", "4L - 2L", &expect!["2"]);
}

#[test]
fn binop_sub_int() {
    check_expr("", "4 - 2", &expect!["2"]);
}

#[test]
fn binop_sub_int_wrap() {
    check_expr(
        "",
        "-0x8000000000000000 - 1",
        &expect!["9223372036854775807"],
    );
}

#[test]
fn binop_sub_double() {
    check_expr("", "4.7 - 2.5", &expect!["2.2"]);
}

#[test]
fn binop_xorb_bigint() {
    check_expr("", "28L ^^^ 54L", &expect!["42"]);
}

#[test]
fn binop_xorb_int() {
    check_expr("", "28 ^^^ 54", &expect!["42"]);
}

#[test]
fn assignop_add_expr() {
    check_expr(
        "",
        indoc! {"{
            mutable x = 0;
            set x += 1;
            x
        }"},
        &expect!["1"],
    );
}

#[test]
fn assignop_sub_expr() {
    check_expr(
        "",
        indoc! {"{
            mutable x = 0;
            set x -= 1;
            x
        }"},
        &expect!["-1"],
    );
}

#[test]
fn assignop_orl_expr() {
    check_expr(
        "",
        indoc! {"{
            mutable x = false;
            set x or= true;
            x
        }"},
        &expect!["true"],
    );
}

#[test]
fn fail_expr() {
    check_expr(
        "",
        r#"fail "This is a failure""#,
        &expect![[r#"
            (
                UserFail(
                    "This is a failure",
                    PackageSpan {
                        package: PackageId(
                            2,
                        ),
                        span: Span {
                            lo: 0,
                            hi: 24,
                        },
                    },
                ),
                [],
            )
        "#]],
    );
}

#[test]
fn fail_shortcut_expr() {
    check_expr(
        "",
        r#"{ fail "Got Here!"; fail "Shouldn't get here..."; }"#,
        &expect![[r#"
            (
                UserFail(
                    "Got Here!",
                    PackageSpan {
                        package: PackageId(
                            2,
                        ),
                        span: Span {
                            lo: 2,
                            hi: 18,
                        },
                    },
                ),
                [],
            )
        "#]],
    );
}

#[test]
fn field_range_start_expr() {
    check_expr("", "(0..2..8)::Start", &expect!["0"]);
}

#[test]
fn field_range_step_expr() {
    check_expr("", "(0..2..8)::Step", &expect!["2"]);
}

#[test]
fn field_range_step_missing_treated_as_1_expr() {
    check_expr("", "(0..8)::Step", &expect!["1"]);
}

#[test]
fn field_range_end_expr() {
    check_expr("", "(0..2..8)::End", &expect!["8"]);
}

#[test]
fn for_loop_range_expr() {
    check_expr(
        "",
        indoc! {"{
            mutable x = 0;
            for i in 0..10 {
                set x = x + i;
            }
            x
        }"},
        &expect!["55"],
    );
}

#[test]
fn for_loop_array_expr() {
    check_expr(
        "",
        indoc! {"{
            mutable x = 0;
            for i in [5, size = 5] {
                set x = x + i;
            }
            x
        }"},
        &expect!["25"],
    );
}

#[test]
fn for_loop_ignore_iterator_expr() {
    check_expr(
        "",
        indoc! {"{
            mutable x = 0;
            for _ in [5, size = 5] {
                set x = x + 1;
            }
            x
        }"},
        &expect!["5"],
    );
}

#[test]
fn array_index_expr() {
    check_expr("", "[1, 2, 3][1]", &expect!["2"]);
}

#[test]
fn array_slice_start_end_expr() {
    check_expr("", "[1, 2, 3, 4, 5][0..2]", &expect!["[1, 2, 3]"]);
}

#[test]
fn array_slice_start_step_end_expr() {
    check_expr("", "[1, 2, 3, 4, 5][0..2..2]", &expect!["[1, 3]"]);
}

#[test]
fn array_slice_start_expr() {
    check_expr("", "[1, 2, 3, 4, 5][2...]", &expect!["[3, 4, 5]"]);
}

#[test]
fn array_slice_end_expr() {
    check_expr("", "[1, 2, 3, 4, 5][...2]", &expect!["[1, 2, 3]"]);
}

#[test]
fn array_slice_step_end_expr() {
    check_expr("", "[1, 2, 3, 4, 5][...2..3]", &expect!["[1, 3]"]);
}

#[test]
fn array_slice_step_expr() {
    check_expr("", "[1, 2, 3, 4, 5][...2...]", &expect!["[1, 3, 5]"]);
}

#[test]
fn array_slice_reverse_expr() {
    check_expr("", "[1, 2, 3, 4, 5][2..-1..0]", &expect!["[3, 2, 1]"]);
}

#[test]
fn array_slice_reverse_end_expr() {
    check_expr("", "[1, 2, 3, 4, 5][...-1..2]", &expect!["[5, 4, 3]"]);
}

#[test]
fn array_slice_reverse_start_expr() {
    check_expr("", "[1, 2, 3, 4, 5][2..-1...]", &expect!["[3, 2, 1]"]);
}

#[test]
fn array_slice_reverse_all_expr() {
    check_expr("", "[1, 2, 3, 4, 5][...-1...]", &expect!["[5, 4, 3, 2, 1]"]);
}

#[test]
fn array_slice_all_expr() {
    check_expr("", "[1, 2, 3, 4, 5][...]", &expect!["[1, 2, 3, 4, 5]"]);
}

#[test]
fn array_slice_none_expr() {
    check_expr("", "[1, 2, 3, 4, 5][1..0]", &expect!["[]"]);
}

#[test]
fn array_slice_reverse_none_expr() {
    check_expr("", "[1, 2, 3, 4, 5][0..-1..1]", &expect!["[]"]);
}

#[test]
fn array_slice_step_zero_expr() {
    check_expr(
        "",
        "[1, 2, 3, 4, 5][...0...]",
        &expect![[r#"
            (
                RangeStepZero(
                    PackageSpan {
                        package: PackageId(
                            2,
                        ),
                        span: Span {
                            lo: 16,
                            hi: 23,
                        },
                    },
                ),
                [],
            )
        "#]],
    );
}

#[test]
fn array_slice_out_of_range_expr() {
    check_expr(
        "",
        "[1, 2, 3, 4, 5][0..7]",
        &expect![[r#"
            (
                IndexOutOfRange(
                    5,
                    PackageSpan {
                        package: PackageId(
                            2,
                        ),
                        span: Span {
                            lo: 16,
                            hi: 20,
                        },
                    },
                ),
                [],
            )
        "#]],
    );
}

#[test]
fn array_index_negative_expr() {
    check_expr(
        "",
        "[1, 2, 3][-2]",
        &expect![[r#"
            (
                InvalidIndex(
                    -2,
                    PackageSpan {
                        package: PackageId(
                            2,
                        ),
                        span: Span {
                            lo: 10,
                            hi: 12,
                        },
                    },
                ),
                [],
            )
        "#]],
    );
}

#[test]
fn array_index_out_of_range_expr() {
    check_expr(
        "",
        "[1, 2, 3][4]",
        &expect![[r#"
            (
                IndexOutOfRange(
                    4,
                    PackageSpan {
                        package: PackageId(
                            2,
                        ),
                        span: Span {
                            lo: 10,
                            hi: 11,
                        },
                    },
                ),
                [],
            )
        "#]],
    );
}

#[test]
fn literal_big_int_expr() {
    check_expr(
        "",
        "9_223_372_036_854_775_808L",
        &expect!["9223372036854775808"],
    );
}

#[test]
fn literal_bool_false_expr() {
    check_expr("", "false", &expect!["false"]);
}

#[test]
fn literal_bool_true_expr() {
    check_expr("", "true", &expect!["true"]);
}

#[test]
fn literal_double_expr() {
    check_expr("", "4.2", &expect!["4.2"]);
}

#[test]
fn literal_double_trailing_dot_expr() {
    check_expr("", "4.", &expect!["4.0"]);
}

#[test]
fn literal_int_expr() {
    check_expr("", "42", &expect!["42"]);
}

#[test]
fn literal_int_too_big_expr() {
    check_expr(
        "",
        "9_223_372_036_854_775_808",
        &expect!["-9223372036854775808"],
    );
}

#[test]
fn literal_pauli_i_expr() {
    check_expr("", "PauliI", &expect!["PauliI"]);
}

#[test]
fn literal_pauli_x_expr() {
    check_expr("", "PauliX", &expect!["PauliX"]);
}

#[test]
fn literal_pauli_y_expr() {
    check_expr("", "PauliY", &expect!["PauliY"]);
}

#[test]
fn literal_pauli_z_expr() {
    check_expr("", "PauliZ", &expect!["PauliZ"]);
}

#[test]
fn literal_result_one_expr() {
    check_expr("", "One", &expect!["One"]);
}

#[test]
fn literal_result_zero_expr() {
    check_expr("", "Zero", &expect!["Zero"]);
}

#[test]
fn literal_string_expr() {
    check_expr("", r#""foo""#, &expect!["foo"]);
}

#[test]
fn literal_tuple_expr() {
    check_expr("", "(1, 2, 3)", &expect!["(1, 2, 3)"]);
}

#[test]
fn literal_tuple_singleton_expr() {
    check_expr("", "(1,)", &expect!["(1,)"]);
}

#[test]
fn literal_tuple_mixed_expr() {
    check_expr(
        "",
        "(1, One, 1.0, [1, 2, 3])",
        &expect!["(1, One, 1.0, [1, 2, 3])"],
    );
}

#[test]
fn paren_expr() {
    check_expr("", "(42)", &expect!["42"]);
}

#[test]
fn range_all_expr() {
    check_expr("", "...", &expect!["..."]);
}

#[test]
fn range_end_expr() {
    check_expr("", "...3", &expect!["...3"]);
}

#[test]
fn range_step_end_expr() {
    check_expr("", "...2..3", &expect!["...2..3"]);
}

#[test]
fn range_start_expr() {
    check_expr("", "1...", &expect!["1..."]);
}

#[test]
fn range_start_end_expr() {
    check_expr("", "1..3", &expect!["1..3"]);
}

#[test]
fn range_start_step_expr() {
    check_expr("", "1..2...", &expect!["1..2..."]);
}

#[test]
fn range_start_step_end_expr() {
    check_expr("", "1..2..3", &expect!["1..2..3"]);
}

#[test]
fn repeat_until_expr() {
    check_expr(
        "",
        indoc! {"{
            mutable x = 0;
            repeat {
                set x = x + 1;
            }
            until x >= 3;
            x
        }"},
        &expect!["3"],
    );
}

#[test]
fn repeat_until_fixup_expr() {
    check_expr(
        "",
        indoc! {"{
            mutable x = 0;
            repeat {}
            until x >= 3
            fixup {
                set x = x + 1;
            }
            x
        }"},
        &expect!["3"],
    );
}

#[test]
fn return_expr() {
    check_expr("", "return 4", &expect!["4"]);
}

#[test]
fn return_shortcut_expr() {
    check_expr(
        "",
        r#"{return 4; fail "Shouldn't get here...";}"#,
        &expect!["4"],
    );
}

#[test]
fn tuple_expr() {
    check_expr("", "(1, 2, 3)", &expect!["(1, 2, 3)"]);
}

#[test]
fn unop_bitwise_not_big_int_expr() {
    check_expr(
        "",
        "~~~(9_223_372_036_854_775_808L)",
        &expect!["-9223372036854775809"],
    );
}

#[test]
fn while_expr() {
    check_expr(
        "",
        indoc! {"{
            mutable x = 0;
            while x < 10 {
                set x = x + 1;
            }
            x
        }"},
        &expect!["10"],
    );
}

#[test]
fn while_false_shortcut_expr() {
    check_expr(
        "",
        r#"while false { fail "Shouldn't fail" }"#,
        &expect!["()"],
    );
}

#[test]
fn cond_expr() {
    check_expr("", "true ? 1 | 0", &expect!["1"]);
}

#[test]
fn cond_false_expr() {
    check_expr("", "false ? 1 | 0", &expect!["0"]);
}

#[test]
fn cond_shortcircuit_expr() {
    check_expr("", r#"true ? 1 | fail "Shouldn't fail""#, &expect!["1"]);
}

#[test]
fn cond_false_shortcircuit_expr() {
    check_expr("", r#"false ? fail "Shouldn't fail" | 0"#, &expect!["0"]);
}

#[test]
fn update_expr() {
    check_expr("", "[1, 2, 3] w/ 2 <- 4", &expect!["[1, 2, 4]"]);
}

#[test]
fn update_invalid_index_range_expr() {
    check_expr(
        "",
        "[1, 2, 3] w/ 7 <- 4",
        &expect![[r#"
            (
                IndexOutOfRange(
                    7,
                    PackageSpan {
                        package: PackageId(
                            2,
                        ),
                        span: Span {
                            lo: 13,
                            hi: 14,
                        },
                    },
                ),
                [],
            )
        "#]],
    );
}

#[test]
fn update_invalid_index_negative_expr() {
    check_expr(
        "",
        "[1, 2, 3] w/ -1 <- 4",
        &expect![[r#"
            (
                InvalidNegativeInt(
                    -1,
                    PackageSpan {
                        package: PackageId(
                            2,
                        ),
                        span: Span {
                            lo: 13,
                            hi: 15,
                        },
                    },
                ),
                [],
            )
        "#]],
    );
}

#[test]
fn update_array_index_var() {
    check_expr(
        "",
        indoc! {"{
            let xs = [2];
            let i = 0;
            xs w/ i <- 3
        }"},
        &expect!["[3]"],
    );
}

#[test]
fn update_array_index_expr() {
    check_expr(
        "",
        indoc! {"{
            let xs = [1, 2];
            let i = 0;
            xs w/ i + 1 <- 3
        }"},
        &expect!["[1, 3]"],
    );
}

#[test]
fn update_udt_known_field_name() {
    check_expr(
        indoc! {"
            namespace A {
                newtype Pair = (First : Int, Second : Int);
            }
        "},
        indoc! {"{
            open A;
            let p = Pair(1, 2);
            p w/ First <- 3
        }"},
        &expect!["(3, 2)"],
    );
}

#[test]
fn update_udt_nested_field() {
    check_expr(
        indoc! {"
            namespace A {
                newtype Triple = (First : Int, (Second : Int, Third : Int));
            }
        "},
        indoc! {"{
            open A;
            let p = Triple(1, (2, 3));
            p w/ Third <- 4
        }"},
        &expect!["(1, (2, 4))"],
    );
}

#[test]
fn update_range_start() {
    check_expr("", "1..2..3 w/ Start <- 10", &expect!["10..2..3"]);
}

#[test]
fn update_range_from_start() {
    check_expr("", "1..2... w/ Start <- 10", &expect!["10..2..."]);
}

#[test]
fn update_range_step() {
    check_expr("", "1..2..3 w/ Step <- 10", &expect!["1..10..3"]);
}

#[test]
fn update_range_from_step() {
    check_expr("", "1..2... w/ Step <- 10", &expect!["1..10..."]);
}

#[test]
fn update_range_to_step() {
    check_expr("", "...2..3 w/ Step <- 10", &expect!["...10..3"]);
}

#[test]
fn update_range_full_step() {
    check_expr("", "...2... w/ Step <- 10", &expect!["...10..."]);
}

#[test]
fn update_range_end() {
    check_expr("", "1..2..3 w/ End <- 10", &expect!["1..2..10"]);
}

#[test]
fn update_range_to_end() {
    check_expr("", "...2..3 w/ End <- 10", &expect!["...2..10"]);
}

#[test]
fn assignupdate_expr() {
    check_expr(
        "",
        indoc! {"{
            mutable x = [1, 2, 3];
            set x w/= 2 <- 4;
            x
        }"},
        &expect!["[1, 2, 4]"],
    );
}

#[test]
fn assignupdate_expr_using_field_name() {
    check_expr(
        indoc! {"
        namespace A {
            newtype Pair = (First : Int, Second : Int);
        }
    "},
        indoc! {"{
            open A;
            mutable p = Pair(1, 2);
            set p w/= First <- 3;
            p
        }"},
        &expect!["(3, 2)"],
    );
}

#[test]
fn unop_bitwise_not_int_expr() {
    check_expr("", "~~~(13)", &expect!["-14"]);
}

#[test]
fn unop_negate_big_int_expr() {
    check_expr(
        "",
        "-(9_223_372_036_854_775_808L)",
        &expect!["-9223372036854775808"],
    );
}

#[test]
fn unop_negate_double_expr() {
    check_expr("", "-(3.4)", &expect!["-3.4"]);
}

#[test]
fn unop_negate_int_expr() {
    check_expr("", "-(13)", &expect!["-13"]);
}

#[test]
fn unop_negate_int_overflow_expr() {
    check_expr(
        "",
        "-(9_223_372_036_854_775_808)",
        &expect!["-9223372036854775808"],
    );
}

#[test]
fn unop_negate_negative_int_expr() {
    check_expr("", "-(-(13))", &expect!["13"]);
}

#[test]
fn unop_not_bool_expr() {
    check_expr("", "not false", &expect!["true"]);
}

#[test]
fn unop_positive_big_int_expr() {
    check_expr(
        "",
        "+(9_223_372_036_854_775_808L)",
        &expect!["9223372036854775808"],
    );
}

#[test]
fn unop_positive_double_expr() {
    check_expr("", "+(3.4)", &expect!["3.4"]);
}

#[test]
fn unop_positive_int_expr() {
    check_expr("", "+(13)", &expect!["13"]);
}

#[test]
fn unop_adjoint_functor_expr() {
    check_expr(
        indoc! {"
            namespace Test {
                operation Foo() : Unit is Adj + Ctl {
                    body ... {}
                }
            }
        "},
        "Adjoint Test.Foo",
        &expect!["Adjoint <item 1 in package 2>"],
    );
}

#[test]
fn unop_controlled_functor_expr() {
    check_expr(
        indoc! {"
            namespace Test {
                operation Foo() : Unit is Adj + Ctl {
                    body ... {}
                }
            }
        "},
        "Controlled Test.Foo",
        &expect!["Controlled <item 1 in package 2>"],
    );
}

#[test]
fn unop_adjoint_adjoint_functor_expr() {
    check_expr(
        indoc! {"
            namespace Test {
                operation Foo() : Unit is Adj + Ctl {
                    body ... {}
                }
            }
        "},
        "Adjoint (Adjoint Test.Foo)",
        &expect!["<item 1 in package 2>"],
    );
}

#[test]
fn unop_controlled_adjoint_functor_expr() {
    check_expr(
        indoc! {"
            namespace Test {
                operation Foo() : Unit is Adj + Ctl {
                    body ... {}
                }
            }
        "},
        "Controlled Adjoint Test.Foo",
        &expect!["Controlled Adjoint <item 1 in package 2>"],
    );
}

#[test]
fn unop_adjoint_controlled_functor_expr() {
    check_expr(
        indoc! {"
            namespace Test {
                operation Foo() : Unit is Adj + Ctl {
                    body ... {}
                }
            }
        "},
        "Adjoint Controlled Test.Foo",
        &expect!["Controlled Adjoint <item 1 in package 2>"],
    );
}

#[test]
fn unop_controlled_controlled_functor_expr() {
    check_expr(
        indoc! {"
            namespace Test {
                operation Foo() : Unit is Adj + Ctl {
                    body ... {}
                }
            }
        "},
        "Controlled (Controlled Test.Foo)",
        &expect!["Controlled Controlled <item 1 in package 2>"],
    );
}

#[test]
fn if_true_expr() {
    check_expr(
        "",
        r#"if true {return "Got Here!";}"#,
        &expect!["Got Here!"],
    );
}

#[test]
fn if_false_expr() {
    check_expr(
        "",
        r#"if false {return "Shouldn't get here...";}"#,
        &expect!["()"],
    );
}

#[test]
fn if_else_true_expr() {
    check_expr(
        "",
        r#"if true {return "Got Here!";} else {return "Shouldn't get here..."}"#,
        &expect!["Got Here!"],
    );
}

#[test]
fn if_else_false_expr() {
    check_expr(
        "",
        r#"if false {return "Shouldn't get here...";} else {return "Got Here!"}"#,
        &expect!["Got Here!"],
    );
}

#[test]
fn if_elif_true_true_expr() {
    check_expr(
        "",
        r#"if true {return "Got Here!";} elif true {return"Shouldn't get here..."}"#,
        &expect!["Got Here!"],
    );
}

#[test]
fn if_elif_false_true_expr() {
    check_expr(
        "",
        r#"if false {return "Shouldn't get here...";} elif true {return "Got Here!"}"#,
        &expect!["Got Here!"],
    );
}

#[test]
fn if_elif_false_false_expr() {
    check_expr(
        "",
        r#"if false {return "Shouldn't get here...";} elif false {return "Shouldn't get here..."}"#,
        &expect!["()"],
    );
}

#[test]
fn if_elif_else_true_true_expr() {
    check_expr(
        "",
        r#"if true {return "Got Here!";} elif true {return "Shouldn't get here..."} else {return "Shouldn't get here..."}"#,
        &expect!["Got Here!"],
    );
}

#[test]
fn if_elif_else_false_true_expr() {
    check_expr(
        "",
        r#"if false {return "Shouldn't get here...";} elif true {return "Got Here!"} else {return "Shouldn't get here..."}"#,
        &expect!["Got Here!"],
    );
}

#[test]
fn if_elif_else_false_false_expr() {
    check_expr(
        "",
        r#"if false {return "Shouldn't get here...";} elif false {return "Shouldn't get here..."} else {return "Got Here!"}"#,
        &expect!["Got Here!"],
    );
}

#[test]
fn call_expr() {
    check_expr(
        indoc! {"
            namespace Test {
                function Answer() : Int {
                    42
                }
            }
        "},
        "Test.Answer()",
        &expect!["42"],
    );
}

#[test]
fn call_return_expr() {
    check_expr(
        indoc! {"
            namespace Test {
                function Answer() : Int {
                    return 42;
                }
            }
        "},
        "Test.Answer()",
        &expect!["42"],
    );
}

#[test]
fn call_args_expr() {
    check_expr(
        indoc! {"
            namespace Test {
                function Echo(val : Int) : Int {
                    return val;
                }
            }
        "},
        "Test.Echo(42)",
        &expect!["42"],
    );
}

#[test]
fn call_multiple_args_expr() {
    check_expr(
        indoc! {"
            namespace Test {
                function Echo(val1 : Int, val2 : Int) : (Int, Int) {
                    return (val1, val2);
                }
            }
        "},
        "Test.Echo(42, 43)",
        &expect!["(42, 43)"],
    );
}

#[test]
fn call_tuple_args_expr() {
    check_expr(
        indoc! {"
            namespace Test {
                function MakeList(val1 : (Int, Int), val2 : Int) : Int[] {
                    let (v1, v2) = val1;
                    return [v1, v2, val2];
                }
            }
        "},
        "Test.MakeList((42, 43), 44)",
        &expect!["[42, 43, 44]"],
    );
}

#[test]
fn call_call_expr() {
    check_expr(
        indoc! {"
            namespace Test {
                function TupleToList(tup : (Int, Int)) : Int[] {
                    let (val, size) = tup;
                    return MakeList(val, size);
                }
                function MakeList(val : Int, size : Int) : Int[] {
                    return [val, size = size];
                }
            }
        "},
        "Test.TupleToList((3, 2))",
        &expect!["[3, 3]"],
    );
}

#[test]
fn call_adjoint_expr() {
    check_expr(
        indoc! {r#"
            namespace Test {
                operation Foo() : Unit is Adj + Ctl {
                    body (...) {
                        fail "Body Implementation";
                    }
                    adjoint (...) {
                        fail "Adjoint Implementation";
                    }
                    controlled (ctls, ...) {
                        fail "Controlled Implementation";
                    }
                    controlled adjoint (ctls, ...) {
                        fail "Controlled Adjoint Implementation";
                    }
                }
            }
        "#},
        "Adjoint Test.Foo()",
        &expect![[r#"
            (
                UserFail(
                    "Adjoint Implementation",
                    PackageSpan {
                        package: PackageId(
                            2,
                        ),
                        span: Span {
                            lo: 185,
                            hi: 214,
                        },
                    },
                ),
                [
                    Frame {
                        span: Span {
                            lo: 190,
                            hi: 214,
                        },
                        id: GlobalId {
                            package: PackageId(
                                2,
                            ),
                            item: LocalItemId(
                                1,
                            ),
                        },
                        caller: PackageId(
                            2,
                        ),
                        functor: FunctorApp {
                            adjoint: true,
                            controlled: 0,
                        },
                    },
                ],
            )
        "#]],
    );
}

#[test]
fn call_adjoint_adjoint_expr() {
    check_expr(
        indoc! {r#"
            namespace Test {
                operation Foo() : Unit is Adj + Ctl {
                    body (...) {
                        fail "Body Implementation";
                    }
                    adjoint (...) {
                        fail "Adjoint Implementation";
                    }
                    controlled (ctls, ...) {
                        fail "Controlled Implementation";
                    }
                    controlled adjoint (ctls, ...) {
                        fail "Controlled Adjoint Implementation";
                    }
                }
            }
        "#},
        "Adjoint Adjoint Test.Foo()",
        &expect![[r#"
            (
                UserFail(
                    "Body Implementation",
                    PackageSpan {
                        package: PackageId(
                            2,
                        ),
                        span: Span {
                            lo: 119,
                            hi: 145,
                        },
                    },
                ),
                [
                    Frame {
                        span: Span {
                            lo: 124,
                            hi: 145,
                        },
                        id: GlobalId {
                            package: PackageId(
                                2,
                            ),
                            item: LocalItemId(
                                1,
                            ),
                        },
                        caller: PackageId(
                            2,
                        ),
                        functor: FunctorApp {
                            adjoint: false,
                            controlled: 0,
                        },
                    },
                ],
            )
        "#]],
    );
}

#[test]
fn call_adjoint_self_expr() {
    check_expr(
        indoc! {r#"
            namespace Test {
                operation Foo() : Unit is Adj + Ctl {
                    body (...) {
                        fail "Body Implementation";
                    }
                    adjoint self;
                    controlled (ctls, ...) {
                        fail "Controlled Implementation";
                    }
                }
            }
        "#},
        "Adjoint Test.Foo()",
        &expect![[r#"
            (
                UserFail(
                    "Body Implementation",
                    PackageSpan {
                        package: PackageId(
                            2,
                        ),
                        span: Span {
                            lo: 111,
                            hi: 137,
                        },
                    },
                ),
                [
                    Frame {
                        span: Span {
                            lo: 116,
                            hi: 137,
                        },
                        id: GlobalId {
                            package: PackageId(
                                2,
                            ),
                            item: LocalItemId(
                                1,
                            ),
                        },
                        caller: PackageId(
                            2,
                        ),
                        functor: FunctorApp {
                            adjoint: true,
                            controlled: 0,
                        },
                    },
                ],
            )
        "#]],
    );
}

#[test]
fn check_ctls_count_expr() {
    check_expr(
        indoc! {r#"
            namespace Test {
                operation Foo() : Unit is Adj + Ctl {
                    body (...) {}
                    adjoint self;
                    controlled (ctls, ...) {
                        if Length(ctls) != 3 {
                            fail "Incorrect ctls count!";
                        }
                    }
                }
            }
        "#},
        indoc! {"
            {
                use qs = Qubit[3];
                Controlled Test.Foo(qs, ());
            }
        "},
        &expect!["()"],
    );
}

#[test]
fn check_ctls_count_nested_expr() {
    check_expr(
        indoc! {r#"
            namespace Test {
                operation Foo() : Unit is Adj + Ctl {
                    body (...) {}
                    adjoint self;
                    controlled (ctls, ...) {
                        if Length(ctls) != 3 {
                            fail "Incorrect ctls count!";
                        }
                    }
                }
            }
        "#},
        indoc! {"
            {
                use qs1 = Qubit[1];
                use qs2 = Qubit[2];
                Controlled Controlled Test.Foo(qs2, (qs1, ()));
            }
        "},
        &expect!["()"],
    );
}

#[test]
fn check_generated_ctl_expr() {
    check_expr(
        indoc! {r#"
            namespace Test {
                operation A() : Unit is Ctl {
                    body ... {}
                    controlled (ctls, ...) {
                        if Length(ctls) != 3 {
                            fail "Incorrect ctls count!";
                        }
                    }
                }
                operation B() : Unit is Ctl {
                    A();
                }
            }
        "#},
        "{use qs = Qubit[3]; Controlled Test.B(qs, ())}",
        &expect!["()"],
    );
}

#[test]
fn check_generated_ctladj_distrib_expr() {
    check_expr(
        indoc! {r#"
            namespace Test {
                operation A() : Unit is Ctl + Adj {
                    body ... { fail "Shouldn't get here"; }
                    adjoint self;
                    controlled (ctls, ...) {
                        if Length(ctls) != 3 {
                            fail "Incorrect ctls count!";
                        }
                    }
                    controlled adjoint (ctls, ...) {
                        if Length(ctls) != 2 {
                            fail "Incorrect ctls count!";
                        }
                    }
                }
                operation B() : Unit is Ctl + Adj {
                    body ... { A(); }
                    adjoint ... { Adjoint A(); }
                }
            }
        "#},
        "{use qs = Qubit[2]; Controlled Adjoint Test.B(qs, ())}",
        &expect!["()"],
    );
}

#[test]
fn global_callable_as_arg() {
    check_expr(
        indoc! {"
            namespace Test {
                function PlusOne(x : Int) : Int {
                    x + 1
                }
                function ApplyToIntArray(f : (Int -> Int)) : Int[] {
                    mutable arr = [1, size = 3];
                    for i in 0..2 {
                        set arr w/= i <- f(arr[i]);
                    }
                    arr
                }
            }
        "},
        "Test.ApplyToIntArray(Test.PlusOne)",
        &expect!["[2, 2, 2]"],
    );
}

#[test]
fn conjugate_output_preserved() {
    check_expr("", "{let x = within{}apply{4}; x}", &expect!["4"]);
}

#[test]
fn interpolated_string() {
    check_expr("", r#"$"string""#, &expect!["string"]);
}

#[test]
fn interpolated_string_var() {
    check_expr(
        "",
        indoc! {r#"{
            let x = 5;
            $"{x}"
        }"#},
        &expect!["5"],
    );
}

#[test]
fn interpolated_string_array_index() {
    check_expr(
        "",
        indoc! {r#"{
            let xs = [1, 2, 3];
            $"{xs[0]}"
        }"#},
        &expect!["1"],
    );
}

#[test]
fn interpolated_string_two_vars() {
    check_expr(
        "",
        indoc! {r#"{
            let x = 4;
            let y = (true, Zero);
            $"{x} {y}"
        }"#},
        &expect!["4 (true, Zero)"],
    );
}

#[test]
fn interpolated_string_nested_normal_string() {
    check_expr("", r#"$"{"{}"}""#, &expect!["{}"]);
}

#[test]
fn nested_interpolated_string() {
    check_expr(
        "",
        indoc! {r#"{
            let x = 4;
            $"{$"{x}"}"
        }"#},
        &expect!["4"],
    );
}

#[test]
fn nested_interpolated_string_with_exprs() {
    check_expr(
        "",
        indoc! {r#"{
            let x = "hello!";
            let y = 1.5;
            $"foo {x + $"bar {y}"} baz"
        }"#},
        &expect!["foo hello!bar 1.5 baz"],
    );
}

#[test]
fn udt_unwrap() {
    check_expr(
        "",
        "{
            newtype Foo = (Int, Bool);
            let foo = Foo(1, true);
            foo!
        }",
        &expect!["(1, true)"],
    );
}

#[test]
fn udt_fields() {
    check_expr(
        "",
        "{
            newtype Point = (X : Int, Y : Int);
            let p = Point(1, 2);
            (p::X, p::Y)
        }",
        &expect!["(1, 2)"],
    );
}

#[test]
fn udt_field_nested() {
    check_expr(
        "",
        "{
            newtype Point = (X : Int, (Y : Int, Z : Int));
            let p = Point(1, (2, 3));
            (p::Y, p::Z)
        }",
        &expect!["(2, 3)"],
    );
}

#[test]
fn lambda_function_empty_closure() {
    check_expr("", "{ let f = x -> x + 1; f(1) }", &expect!["2"]);
}

#[test]
fn lambda_function_empty_closure_passed() {
    check_expr(
        "",
        "{ function Foo(f : Int -> Int) : Int { f(2) }; Foo(x -> x + 1) }",
        &expect!["3"],
    );
}

#[test]
fn lambda_function_closure() {
    check_expr(
        "",
        "{ let x = 5; let f = y -> (x, y); f(2) }",
        &expect!["(5, 2)"],
    );
}

#[test]
fn lambda_function_closure_passed() {
    check_expr(
        "",
        "{ function Foo(f : Int -> (Int, Int)) : (Int, Int) { f(2) }; let x = 5; Foo(y -> (x, y)) }",
        &expect!["(5, 2)"],
    );
}

#[test]
fn lambda_function_nested_closure() {
    check_expr(
        "
            namespace A {
                function Foo(f : Int -> Int -> (Int, Int, Int, Int)) : (Int, Int, Int, Int) {
                    f(2)(3)
                }

                function Bar() : (Int, Int, Int, Int) {
                    let a = 5;
                    Foo(b -> {
                        let c = 1;
                        d -> (a, b, c, d)
                    })
                }
            }
        ",
        "A.Bar()",
        &expect!["(5, 2, 1, 3)"],
    );
}

#[test]
fn lambda_operation_empty_closure() {
    check_expr(
        "
            namespace A {
                open Microsoft.Quantum.Measurement;

                operation Foo(op : Qubit => ()) : Result {
                    use q = Qubit();
                    op(q);
                    MResetZ(q)
                }

                operation Bar() : Result { Foo(q => X(q)) }
            }
        ",
        "A.Bar()",
        &expect!["One"],
    );
}

#[test]
fn lambda_operation_closure() {
    check_expr(
        "
            namespace A {
                open Microsoft.Quantum.Measurement;
                operation Foo(op : () => Result) : Result { op() }
                operation Bar() : Result {
                    use q = Qubit();
                    X(q);
                    Foo(() => MResetZ(q))
                }
            }
        ",
        "A.Bar()",
        &expect!["One"],
    );
}

#[test]
fn lambda_operation_controlled() {
    check_expr(
        "
            namespace A {
                open Microsoft.Quantum.Measurement;
                operation Foo(op : Qubit => Unit is Adj + Ctl, q : Qubit) : Unit is Adj + Ctl { op(q) }
                operation Bar() : Result[] {
                    mutable output = [];
                    use (ctls, q) = (Qubit[1], Qubit());
                    let op = q => X(q);
                    Foo(op, q);
                    set output += [MResetZ(q)];
                    Controlled Foo(ctls, (op, q));
                    set output += [MResetZ(q)];
                    X(ctls[0]);
                    Controlled Foo(ctls, (op, q));
                    set output += [MResetZ(q)];
                    ResetAll(ctls);
                    output
                }
            }
        ",
        "A.Bar()",
        &expect!["[One, Zero, One]"],
    );
}

#[test]
fn lambda_operation_controlled_controlled() {
    check_expr(
        "
            namespace A {
                open Microsoft.Quantum.Measurement;
                operation Foo(op : Qubit => Unit is Adj + Ctl, q : Qubit) : Unit is Adj + Ctl { op(q) }
                operation Bar() : Result[] {
                    mutable output = [];
                    use (ctls1, ctls2, q) = (Qubit[1], Qubit[1], Qubit());
                    let op = q => X(q);
                    Foo(op, q);
                    set output += [MResetZ(q)];
                    Controlled Controlled Foo(ctls1, (ctls2, (op, q)));
                    set output += [MResetZ(q)];
                    X(ctls1[0]);
                    X(ctls2[0]);
                    Controlled Controlled Foo(ctls1, (ctls2, (op, q)));
                    set output += [MResetZ(q)];
                    ResetAll(ctls1 + ctls2);
                    output
                }
            }
        ",
        "A.Bar()",
        &expect!["[One, Zero, One]"],
    );
}

#[test]
fn partial_app_all_holes() {
    check_expr(
        "",
        "{
            function F(x : Int, y : Int) : Int { x + y }
            let f = F(_, _);
            f(1, 2)
        }",
        &expect!["3"],
    );
}

#[test]
fn partial_app_one_fixed_arg() {
    check_expr(
        "",
        "{
            function F(x : Int, y : Int) : Int { x + y }
            let f = F(_, 2);
            f(1)
        }",
        &expect!["3"],
    );
}

#[test]
fn partial_app_nested_tuple() {
    check_expr(
        "",
        "{
            function F(a : Int, (b : Int, c : Int, d : Int)) : (Int, Int, Int, Int) { (a, b, c, d) }
            let f = F(_, (_, 3, _));
            f(1, (2, 4))
        }",
        &expect!["(1, 2, 3, 4)"],
    );
}

#[test]
fn partial_app_arg_with_side_effect() {
    check_expr(
        "",
        "{
            operation F(_ : (), x : Int) : Int { x }
            use q = Qubit();
            let f = F(X(q), _);
            let r1 = M(q);
            f(1);
            let r2 = M(q);
            f(2);
            let r3 = M(q);
            Reset(q);
            (r1, r2, r3)
        }",
        &expect!["(One, One, One)"],
    );
}

#[test]
fn partial_app_mutable_arg() {
    check_expr(
        "",
        "{
            function F(a : Int, b : Int) : (Int, Int) { (a, b) }
            mutable x = 0;
            let f = F(x, _);
            let r1 = f(1);
            set x = 1;
            let r2 = f(2);
            (r1, r2)
        }",
        &expect!["((0, 1), (0, 2))"],
    );
}
