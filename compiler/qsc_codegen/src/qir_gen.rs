// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use qsc_fir::{fir::{ExprId, Package, BlockId, PatId, StmtId, Block, Expr, Pat, Stmt}, visit::Visitor};

#[must_use]
pub fn generate_qir(package: &Package, expr: ExprId) -> String {
    let mut gen = Generator {
        package,
        qir: String::new(),
    };

    gen.visit_expr(expr);

    gen.qir
}

struct Generator<'a> {
    package: &'a Package,
    qir: String,
}

impl<'a> Visitor<'a> for Generator<'a> {
    fn get_block(&mut self, id: BlockId) -> &'a Block {
        self.package.blocks.get(id).expect("block not found")
    }

    fn get_expr(&mut self, id: ExprId) -> &'a Expr {
        self.package.exprs.get(id).expect("expr not found")
    }

    fn get_pat(&mut self, id: PatId) -> &'a Pat {
        self.package.pats.get(id).expect("pat not found")
    }

    fn get_stmt(&mut self, id: StmtId) -> &'a Stmt {
        self.package.stmts.get(id).expect("stmt not found")
    }
}
