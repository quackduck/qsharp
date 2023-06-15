// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::test_expression;
use qsc::interpret::Value;

// Tests for Microsoft.Quantum.Convert namespace

#[test]
fn check_bool_array_as_int() {
    test_expression(
        "Microsoft.Quantum.Convert.BoolArrayAsInt([true, false, true, false])",
        &Value::Int(0b0101),
    );
}

#[test]
fn check_int_as_bool_array() {
    test_expression(
        "Microsoft.Quantum.Convert.IntAsBoolArray(5,4)",
        &Value::Array(
            vec![
                Value::Bool(true),
                Value::Bool(false),
                Value::Bool(true),
                Value::Bool(false),
            ]
            .into(),
        ),
    );
}

#[test]
fn check_result_array_as_int() {
    test_expression(
        "Microsoft.Quantum.Convert.ResultArrayAsInt([One, Zero, One, Zero])",
        &Value::Int(0b0101),
    );
}

#[test]
fn check_result_array_as_bool_array() {
    test_expression(
        "Microsoft.Quantum.Convert.ResultArrayAsBoolArray([One, Zero, One, Zero])",
        &Value::Array(
            vec![
                Value::Bool(true),
                Value::Bool(false),
                Value::Bool(true),
                Value::Bool(false),
            ]
            .into(),
        ),
    );
}

#[test]
fn check_bool_array_as_result_array() {
    test_expression(
        "Microsoft.Quantum.Convert.BoolArrayAsResultArray([true, false, true, false])",
        &Value::Array(
            vec![
                Value::Result(true),
                Value::Result(false),
                Value::Result(true),
                Value::Result(false),
            ]
            .into(),
        ),
    );
}