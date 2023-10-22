// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::fmt::{Display, Write};

use qsc_eval::val::Value;

pub(crate) struct Qubit(pub usize);

impl Display for Qubit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "%Qubit* inttoptr (i64 {} to %Qubit*)", self.0)
    }
}

pub(crate) struct Result(pub usize);

impl Display for Result {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "%Result* inttoptr (i64 {} to %Result*)", self.0)
    }
}

pub(crate) struct Double(pub f64);

impl Display for Double {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let v = self.0;
        if (v.floor() - v.ceil()).abs() < f64::EPSILON {
            // The value is a whole number, which requires at least one decimal point
            // to differentiate it from an integer value.
            write!(f, "double {v:.1}")
        } else {
            write!(f, "double {v}")
        }
    }
}

pub(crate) struct Int(pub i64);

impl Display for Int {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "i64 {}", self.0)
    }
}

pub(crate) fn write_output_recording(w: &mut impl Write, val: &Value) {
    match val {
        Value::Array(arr) => {
            write_array_recording(w, arr.len());
            for val in arr.iter() {
                write_output_recording(w, val);
            }
        }
        Value::Result(r) => {
            write_result_recording(w, r.unwrap_id());
        }
        Value::Tuple(tup) => {
            write_tuple_recording(w, tup.len());
            for val in tup.iter() {
                write_output_recording(w, val);
            }
        }
        _ => panic!("unexpected value type: {val:?}"),
    }
}

fn write_result_recording(w: &mut impl Write, res: usize) {
    writeln!(
        w,
        "  call void @__quantum__rt__result_record_output({}, i8* null)",
        Result(res),
    )
    .expect("writing to string should succeed");
}

fn write_tuple_recording(w: &mut impl Write, size: usize) {
    #[allow(clippy::cast_possible_wrap)]
    let size = Int(size as i64);
    writeln!(
        w,
        "  call void @__quantum__rt__tuple_record_output({size}, i8* null)",
    )
    .expect("writing to string should succeed");
}

fn write_array_recording(w: &mut impl Write, size: usize) {
    #[allow(clippy::cast_possible_wrap)]
    let size = Int(size as i64);
    writeln!(
        w,
        "  call void @__quantum__rt__array_record_output({size}, i8* null)"
    )
    .expect("writing to string should succeed");
}

pub(crate) fn write_ccx(w: &mut impl Write, ctl0: usize, ctl1: usize, q: usize) {
    writeln!(
        w,
        "  call void @__quantum__qis__ccx__body({}, {}, {})",
        Qubit(ctl0),
        Qubit(ctl1),
        Qubit(q)
    )
    .expect("writing to string should succeed");
}

pub(crate) fn write_cx(w: &mut impl Write, ctl: usize, q: usize) {
    writeln!(
        w,
        "  call void @__quantum__qis__cx__body({}, {})",
        Qubit(ctl),
        Qubit(q),
    )
    .expect("writing to string should succeed");
}

pub(crate) fn write_cy(w: &mut impl Write, ctl: usize, q: usize) {
    writeln!(
        w,
        "  call void @__quantum__qis__cy__body({}, {})",
        Qubit(ctl),
        Qubit(q),
    )
    .expect("writing to string should succeed");
}

pub(crate) fn write_cz(w: &mut impl Write, ctl: usize, q: usize) {
    writeln!(
        w,
        "  call void @__quantum__qis__cz__body({}, {})",
        Qubit(ctl),
        Qubit(q),
    )
    .expect("writing to string should succeed");
}

pub(crate) fn write_h(w: &mut impl Write, q: usize) {
    writeln!(w, "  call void @__quantum__qis__h__body({})", Qubit(q),)
        .expect("writing to string should succeed");
}

pub(crate) fn write_mz(w: &mut impl Write, q: usize, r: usize) {
    writeln!(
        w,
        "  call void @__quantum__qis__mz__body({}, {}) #1",
        Qubit(q),
        Result(r),
    )
    .expect("writing to string should succeed");
}

pub(crate) fn write_rx(w: &mut impl Write, theta: f64, q: usize) {
    writeln!(
        w,
        "  call void @__quantum__qis__rx__body({}, {})",
        Double(theta),
        Qubit(q),
    )
    .expect("writing to string should succeed");
}

pub(crate) fn write_rxx(w: &mut impl Write, theta: f64, q0: usize, q1: usize) {
    writeln!(
        w,
        "  call void @__quantum__qis__rxx__body({}, {}, {})",
        Double(theta),
        Qubit(q0),
        Qubit(q1),
    )
    .expect("writing to string should succeed");
}

pub(crate) fn write_ry(w: &mut impl Write, theta: f64, q: usize) {
    writeln!(
        w,
        "  call void @__quantum__qis__ry__body({}, {})",
        Double(theta),
        Qubit(q),
    )
    .expect("writing to string should succeed");
}

pub(crate) fn write_ryy(w: &mut impl Write, theta: f64, q0: usize, q1: usize) {
    writeln!(
        w,
        "  call void @__quantum__qis__ryy__body({}, {}, {})",
        Double(theta),
        Qubit(q0),
        Qubit(q1),
    )
    .expect("writing to string should succeed");
}

pub(crate) fn write_rz(w: &mut impl Write, theta: f64, q: usize) {
    writeln!(
        w,
        "  call void @__quantum__qis__rz__body({}, {})",
        Double(theta),
        Qubit(q),
    )
    .expect("writing to string should succeed");
}

pub(crate) fn write_rzz(w: &mut impl Write, theta: f64, q0: usize, q1: usize) {
    writeln!(
        w,
        "  call void @__quantum__qis__rzz__body({}, {}, {})",
        Double(theta),
        Qubit(q0),
        Qubit(q1),
    )
    .expect("writing to string should succeed");
}

pub(crate) fn write_sadj(w: &mut impl Write, q: usize) {
    writeln!(w, "  call void @__quantum__qis__s__adj({})", Qubit(q),)
        .expect("writing to string should succeed");
}

pub(crate) fn write_s(w: &mut impl Write, q: usize) {
    writeln!(w, "  call void @__quantum__qis__s__body({})", Qubit(q),)
        .expect("writing to string should succeed");
}

pub(crate) fn write_swap(w: &mut impl Write, q0: usize, q1: usize) {
    writeln!(
        w,
        "  call void @__quantum__qis__swap__body({}, {})",
        Qubit(q0),
        Qubit(q1),
    )
    .expect("writing to string should succeed");
}

pub(crate) fn write_tadj(w: &mut impl Write, q: usize) {
    writeln!(w, "  call void @__quantum__qis__t__adj({})", Qubit(q),)
        .expect("writing to string should succeed");
}

pub(crate) fn write_t(w: &mut impl Write, q: usize) {
    writeln!(w, "  call void @__quantum__qis__t__body({})", Qubit(q),)
        .expect("writing to string should succeed");
}

pub(crate) fn write_x(w: &mut impl Write, q: usize) {
    writeln!(w, "  call void @__quantum__qis__x__body({})", Qubit(q),)
        .expect("writing to string should succeed");
}

pub(crate) fn write_y(w: &mut impl Write, q: usize) {
    writeln!(w, "  call void @__quantum__qis__y__body({})", Qubit(q),)
        .expect("writing to string should succeed");
}

pub(crate) fn write_z(w: &mut impl Write, q: usize) {
    writeln!(w, "  call void @__quantum__qis__z__body({})", Qubit(q),)
        .expect("writing to string should succeed");
}
