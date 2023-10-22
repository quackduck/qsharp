// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::fmt::Display;

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

pub(crate) fn write_output_recording(w: &mut impl std::fmt::Write, val: &Value) {
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

fn write_result_recording(w: &mut impl std::fmt::Write, res: usize) {
    writeln!(
        w,
        "  call void @__quantum__rt__result_record_output({}, i8* null)",
        Result(res),
    )
    .expect("writing to string should succeed");
}

fn write_tuple_recording(w: &mut impl std::fmt::Write, size: usize) {
    #[allow(clippy::cast_possible_wrap)]
    let size = Int(size as i64);
    writeln!(
        w,
        "  call void @__quantum__rt__tuple_record_output({size}, i8* null)",
    )
    .expect("writing to string should succeed");
}

fn write_array_recording(w: &mut impl std::fmt::Write, size: usize) {
    #[allow(clippy::cast_possible_wrap)]
    let size = Int(size as i64);
    writeln!(
        w,
        "  call void @__quantum__rt__array_record_output({size}, i8* null)"
    )
    .expect("writing to string should succeed");
}
