//! Additional tests for semantic rules that the provided test inputs don't cover.

/// Parses and analyzes `input`, returning the error message on failure.
fn analyze(input: &str) -> Result<(), String> {
    let mut ast = c1::parse(input).expect("parsing failed");
    c1::analyze(&mut ast).map(|_| ()).map_err(|err| err.to_string())
}

#[track_caller]
fn assert_ok(input: &str) {
    if let Err(err) = analyze(input) {
        panic!("analysis failed: {err}\ninput: {input}");
    }
}

#[track_caller]
fn assert_err(input: &str, expected: &str) {
    assert_eq!(analyze(input), Err(expected.to_owned()), "input: {input}");
}

// 1.2 Functions are visible in the whole program, even before their definition.
#[test]
fn forward_call() {
    assert_ok("void main() { print(later(1)); } int later(int x) { return x; }");
}

// 1.3 Variables must be declared before they are used, globals included.
#[test]
fn global_used_before_definition() {
    assert_err(
        "void main() { print(g); } int g = 1;",
        "cannot resolve `g` in this scope",
    );
    assert_ok("int g = 1; void main() { print(g); }");
}

#[test]
fn local_used_before_definition() {
    assert_err(
        "void main() { x = 1; int x; }",
        "cannot resolve `x` in this scope",
    );
}

// 1.4 An identifier may be declared at most once per scope.
#[test]
fn duplicate_definitions() {
    assert_err(
        "int g; int g; void main() {}",
        "duplicate definition of `g` in the same scope",
    );
    assert_err(
        "int f() { return 1; } void main() {} int f() { return 2; }",
        "duplicate definition of `f` in the same scope",
    );
    assert_err(
        "int f; int f() { return 1; } void main() {}",
        "duplicate definition of `f` in the same scope",
    );
    assert_err(
        "void f(int a, float a) {} void main() {}",
        "duplicate definition of `a` in the same scope",
    );
}

// 1.5 Names are resolved from the innermost to the outermost scope.
#[test]
fn shadowing_in_nested_scopes() {
    assert_ok(
        "int x = 1;
         void main() {
             float x = 2.0;
             { bool x = true; if (x) print(x); }
             for (int x = 0; x < 3; x = x + 1) { bool x = false; print(x); }
             x = 3.5;
         }",
    );
}

#[test]
fn block_and_for_scopes_end() {
    assert_err(
        "void main() { { int x; } x = 1; }",
        "cannot resolve `x` in this scope",
    );
    assert_err(
        "void main() { for (int i = 0; i < 3; i = i + 1) ; print(i); }",
        "cannot resolve `i` in this scope",
    );
}

// A function's name belongs to the outer scope, so it may be shadowed inside.
#[test]
fn local_shadows_function_name() {
    assert_ok("int f() { int f = 1; return f; } void main() {}");
}

// 2. `int` converts implicitly to `float`, but not the other way round.
#[test]
fn int_to_float_conversions() {
    assert_ok(
        "float half(float x) { return x / 2; }
         float one() { return 1; }
         void main() { float f = 1; f = 2; print(half(3)); }",
    );
    assert_err(
        "int f() { return 1.0; } void main() {}",
        "cannot return value of type `float` from function returning `int`",
    );
}

// 2.3 No variables of type `void`, even without initializer.
#[test]
fn void_for_var() {
    assert_err(
        "void main() { for (void i; true; i = 1) ; }",
        "cannot define local variable `i` with type `void`",
    );
}

// 2.4 The types of all arguments are checked, not just the first.
#[test]
fn second_argument_mismatch() {
    assert_err(
        "void f(int a, bool b) {} void main() { f(1, 2); }",
        "incorrect type for argument 1 in call to `f`, expected `bool`, found `int`",
    );
    assert_err(
        "void f(int a) {} void main() { f(1, 2); }",
        "incorrect number of arguments in call to `f`, expected 1, found 2",
    );
}

// 2.6 `void` functions may use `return;` without a value.
#[test]
fn empty_return_in_void() {
    assert_ok("void main() { return; }");
}

// 3.3 Comparisons: compatible, non-`void` operands; `int` and `float` mix.
#[test]
fn comparisons() {
    assert_ok(
        "void main() { bool b = 1 < 2.5; b = true == false; b = \"a\" != \"b\"; b = 1.0 >= 1; }",
    );
    assert_err(
        "void main() { bool b = 1 == true; }",
        "cannot apply binary operator to incompatible types: `int == bool`",
    );
}

// 3.4 Logical operators need `bool` operands.
#[test]
fn logical_operators() {
    // `||` binds tighter than `<` in C1, so the comparison needs parentheses.
    assert_ok("void main() { bool b = true && false || (1 < 2); }");
    assert_err(
        "void main() { bool b = 1.0 || 2.0; }",
        "cannot use logical operator `||` with values of type `float`",
    );
}

// 3.5 Arithmetic operators need `int` or `float` operands.
#[test]
fn arithmetic_on_strings() {
    assert_err(
        "void main() { print(\"a\" + \"b\"); }",
        "cannot use arithmetic operator `+` with values of type `string`",
    );
    assert_err(
        "void main() { print(-\"a\"); }",
        "cannot apply unary minus to type `string`",
    );
}

// 3.6 The type of an assignment is the type of its variable.
#[test]
fn assignment_expression_type() {
    assert_ok("void main() { float f; int i; f = i = 1; }");
    assert_err(
        "void main() { float f; int i; i = f = 1; }",
        "cannot assign value of type `float` to variable `i` of type `int`",
    );
    assert_err(
        "void main() { int i; if (i = 1) ; }",
        "condition of if must have type `bool`, found `int`",
    );
}

// 3.8 Arithmetic results are `float` if one operand is `float`, else `int`.
#[test]
fn arithmetic_result_types() {
    assert_ok("void main() { int i = 1 + 2 * 3 / 4 - -5; float f = 1 + 2.0; }");
    assert_err(
        "void main() { int i = 1 * 2.0; }",
        "cannot initialize variable `i` of type `int` with value of type `float`",
    );
}

// Calling a function whose name is shadowed by a local variable fails.
#[test]
fn call_shadowed_function() {
    assert_err(
        "int f() { return 1; } void main() { int f = 1; f(); }",
        "cannot call variable `f`",
    );
}
