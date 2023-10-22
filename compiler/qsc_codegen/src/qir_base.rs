// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(test)]
mod tests;

use crate::qir_writer::{
    write_ccx, write_cx, write_cy, write_cz, write_h, write_mz, write_output_recording, write_rx,
    write_rxx, write_ry, write_ryy, write_rz, write_rzz, write_s, write_sadj, write_swap, write_t,
    write_tadj, write_x, write_y, write_z,
};
use num_bigint::BigUint;
use num_complex::Complex;
use qsc_data_structures::index_map::IndexMap;
use qsc_eval::{
    backend::Backend,
    debug::{map_hir_package_to_fir, Frame},
    eval_expr,
    output::GenericReceiver,
    val::{GlobalId, Value},
    Env, Error, Global, NodeLookup, State,
};
use qsc_fir::fir::{BlockId, ExprId, ItemKind, PackageId, PatId, StmtId};
use qsc_frontend::compile::PackageStore;
use qsc_hir::hir::{self};
use std::fmt::Write;

/// # Errors
///
/// This function will return an error if execution was unable to complete.
/// # Panics
///
/// This function will panic if compiler state is invalid or in out-of-memory conditions.
pub fn generate_qir(
    store: &PackageStore,
    package: hir::PackageId,
) -> std::result::Result<String, (Error, Vec<Frame>)> {
    let mut fir_lowerer = qsc_eval::lower::Lowerer::new();
    let mut fir_store = IndexMap::new();
    let package = map_hir_package_to_fir(package);
    let mut sim = BaseProfSim::default();

    for (id, unit) in store.iter() {
        fir_store.insert(
            map_hir_package_to_fir(id),
            fir_lowerer.lower_package(&unit.package),
        );
    }

    let unit = fir_store.get(package).expect("store should have package");
    let entry_expr = unit.entry.expect("package should have entry");

    let mut stdout = std::io::sink();
    let mut out = GenericReceiver::new(&mut stdout);
    let result = eval_expr(
        &mut State::new(package),
        entry_expr,
        &Lookup {
            fir_store: &fir_store,
        },
        &mut Env::with_empty_scope(),
        &mut sim,
        &mut out,
    );
    match result {
        Ok(val) => Ok(sim.finish(&val)),
        Err((err, stack)) => Err((err, stack)),
    }
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

pub struct BaseProfSim {
    next_meas_id: usize,
    next_qubit_id: usize,
    qubit_map: IndexMap<usize, usize>,
    instrs: String,
    measurements: String,
}

impl Default for BaseProfSim {
    fn default() -> Self {
        Self::new()
    }
}

impl BaseProfSim {
    #[must_use]
    pub fn new() -> Self {
        let mut sim = BaseProfSim {
            next_meas_id: 0,
            next_qubit_id: 0,
            qubit_map: IndexMap::new(),
            instrs: String::new(),
            measurements: String::new(),
        };
        sim.instrs.push_str(include_str!("./qir_base/prefix.ll"));
        sim
    }

    #[must_use]
    pub fn finish(mut self, val: &Value) -> String {
        self.instrs.push_str(&self.measurements);
        write_output_recording(&mut self.instrs, val);

        write!(
            self.instrs,
            include_str!("./qir_base/postfix.ll"),
            self.next_qubit_id, self.next_meas_id
        )
        .expect("writing to string should succeed");

        self.instrs
    }

    #[must_use]
    fn get_meas_id(&mut self) -> usize {
        let id = self.next_meas_id;
        self.next_meas_id += 1;
        id
    }

    fn map(&mut self, qubit: usize) -> usize {
        if let Some(mapped) = self.qubit_map.get(qubit) {
            *mapped
        } else {
            let mapped = self.next_qubit_id;
            self.next_qubit_id += 1;
            self.qubit_map.insert(qubit, mapped);
            mapped
        }
    }
}

impl Backend for BaseProfSim {
    type ResultType = usize;

    fn ccx(&mut self, ctl0: usize, ctl1: usize, q: usize) {
        let ctl0 = self.map(ctl0);
        let ctl1 = self.map(ctl1);
        let q = self.map(q);
        write_ccx(&mut self.instrs, ctl0, ctl1, q);
    }

    fn cx(&mut self, ctl: usize, q: usize) {
        let ctl = self.map(ctl);
        let q = self.map(q);
        write_cx(&mut self.instrs, ctl, q);
    }

    fn cy(&mut self, ctl: usize, q: usize) {
        let ctl = self.map(ctl);
        let q = self.map(q);
        write_cy(&mut self.instrs, ctl, q);
    }

    fn cz(&mut self, ctl: usize, q: usize) {
        let ctl = self.map(ctl);
        let q = self.map(q);
        write_cz(&mut self.instrs, ctl, q);
    }

    fn h(&mut self, q: usize) {
        let q = self.map(q);
        write_h(&mut self.instrs, q);
    }

    fn m(&mut self, q: usize) -> Self::ResultType {
        let q = self.map(q);
        let id = self.get_meas_id();
        // Measurements are tracked separately from instructions, so that they can be
        // deferred until the end of the program.
        write_mz(&mut self.measurements, q, id);
        id
    }

    fn mresetz(&mut self, q: usize) -> Self::ResultType {
        let id = self.m(q);
        self.reset(q);
        id
    }

    fn reset(&mut self, q: usize) {
        // Reset is a no-op in Base Profile, but does force qubit remapping so that future
        // operations on the given qubit id are performed on a fresh qubit. Clear the entry in the map
        // so it is known to require remapping on next use.
        self.qubit_map.remove(q);
    }

    fn rx(&mut self, theta: f64, q: usize) {
        let q = self.map(q);
        write_rx(&mut self.instrs, theta, q);
    }

    fn rxx(&mut self, theta: f64, q0: usize, q1: usize) {
        let q0 = self.map(q0);
        let q1 = self.map(q1);
        write_rxx(&mut self.instrs, theta, q0, q1);
    }

    fn ry(&mut self, theta: f64, q: usize) {
        let q = self.map(q);
        write_ry(&mut self.instrs, theta, q);
    }

    fn ryy(&mut self, theta: f64, q0: usize, q1: usize) {
        let q0 = self.map(q0);
        let q1 = self.map(q1);
        write_ryy(&mut self.instrs, theta, q0, q1);
    }

    fn rz(&mut self, theta: f64, q: usize) {
        let q = self.map(q);
        write_rz(&mut self.instrs, theta, q);
    }

    fn rzz(&mut self, theta: f64, q0: usize, q1: usize) {
        let q0 = self.map(q0);
        let q1 = self.map(q1);
        write_rzz(&mut self.instrs, theta, q0, q1);
    }

    fn sadj(&mut self, q: usize) {
        let q = self.map(q);
        write_sadj(&mut self.instrs, q);
    }

    fn s(&mut self, q: usize) {
        let q = self.map(q);
        write_s(&mut self.instrs, q);
    }

    fn swap(&mut self, q0: usize, q1: usize) {
        let q0 = self.map(q0);
        let q1 = self.map(q1);
        write_swap(&mut self.instrs, q0, q1);
    }

    fn tadj(&mut self, q: usize) {
        let q = self.map(q);
        write_tadj(&mut self.instrs, q);
    }

    fn t(&mut self, q: usize) {
        let q = self.map(q);
        write_t(&mut self.instrs, q);
    }

    fn x(&mut self, q: usize) {
        let q = self.map(q);
        write_x(&mut self.instrs, q);
    }

    fn y(&mut self, q: usize) {
        let q = self.map(q);
        write_y(&mut self.instrs, q);
    }

    fn z(&mut self, q: usize) {
        let q = self.map(q);
        write_z(&mut self.instrs, q);
    }

    fn qubit_allocate(&mut self) -> usize {
        let id = self.next_qubit_id;
        self.next_qubit_id += 1;
        self.qubit_map.insert(id, id);
        id
    }

    fn qubit_release(&mut self, _q: usize) {
        // Base Profile qubits are never released, since they cannot be reused.
    }

    fn capture_quantum_state(&mut self) -> (Vec<(BigUint, Complex<f64>)>, usize) {
        (Vec::new(), 0)
    }

    fn qubit_is_zero(&mut self, _q: usize) -> bool {
        // Because `qubit_is_zero` is called on every qubit release, this must return
        // true to avoid a panic.
        true
    }

    fn reinit(&mut self) {
        *self = Self::default();
    }
}
