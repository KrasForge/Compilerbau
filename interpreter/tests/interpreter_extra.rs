//! Additional tests for runtime semantics that the provided test inputs don't cover.

/// Parses, analyzes and runs `input`, returning the output or error message.
fn run(input: &str) -> Result<String, String> {
    let mut ast = c1::parse(input).expect("parsing failed");
    let analysis = c1::analyze(&mut ast).expect("analysis failed");
    c1::interpret(&ast, &analysis).map_err(|err| err.to_string())
}

#[track_caller]
fn assert_output(input: &str, expected: &str) {
    assert_eq!(run(input), Ok(expected.to_owned()), "input: {input}");
}

#[track_caller]
fn assert_error(input: &str, expected: &str) {
    assert_eq!(run(input), Err(expected.to_owned()), "input: {input}");
}

// Global variables are initialized in declaration order before `main` runs.
#[test]
fn global_initialization_order() {
    assert_output(
        "int trace(int x) { print(\"init \", x); return x; }
         int a = trace(1);
         int b = trace(a + 1);
         void main() { print(\"main \", a, \" \", b); }",
        "init 1\ninit 2\nmain 1 2\n",
    );
}

// Arguments and operands are evaluated from left to right.
#[test]
fn evaluation_order() {
    assert_output(
        "int trace(int x) { print(x); return x; }
         int sum(int a, int b, int c) { return a + b + c; }
         void main() {
             print(sum(trace(1), trace(2), trace(3)));
             print(trace(4) * trace(5) - trace(6));
         }",
        "1\n2\n3\n6\n4\n5\n6\n14\n",
    );
}

// Assignments take effect right after their right hand side was evaluated.
#[test]
fn assignment_takes_effect_immediately() {
    assert_output(
        "void main() { int x = 1; print((x = x + 1) * x); }",
        "4\n",
    );
}

// `&&` and `||` short-circuit as in C.
#[test]
fn logical_operators_short_circuit() {
    assert_output(
        "bool trace(bool b) { print(\"eval \", b); return b; }
         void main() {
             print(trace(false) && trace(true));
             print(trace(true) || trace(false));
             print(trace(true) && trace(false));
             print(trace(false) || trace(true));
         }",
        "eval false\nfalse\neval true\ntrue\neval true\neval false\nfalse\neval false\neval true\ntrue\n",
    );
    // The right operand would divide by zero if it were evaluated.
    // (`&&` binds tighter than comparisons in C1, hence the parentheses.)
    assert_output(
        "void main() { int x = 0; print((x != 0) && (1 / x > 0)); }",
        "false\n",
    );
}

// `return` leaves the function from nested loops and blocks.
#[test]
fn return_from_nested_loops() {
    // Searches `i * j == n` with `1 <= j <= i < 10` using all three loop kinds.
    assert_output(
        "int find(int n) {
             for (int i = 1; i < 10; i = i + 1) {
                 int j = 1;
                 while (j <= i) {
                     do {
                         { if (i * j == n) return i * 10 + j; }
                         j = j + 1;
                     } while (false);
                 }
             }
             return -1;
         }
         void main() { print(find(12)); print(find(11)); }",
        "43\n-1\n",
    );
}

#[test]
fn do_while_runs_at_least_once() {
    assert_output(
        "void main() { int i = 5; do { print(i); i = i + 1; } while (i < 3); }",
        "5\n",
    );
}

#[test]
fn for_with_assignment_init() {
    assert_output(
        "void main() { int i; for (i = 3; i > 0; i = i - 1) print(i); print(i); }",
        "3\n2\n1\n0\n",
    );
}

#[test]
fn dangling_else() {
    assert_output(
        "void main() { if (true) if (false) print(1); else print(2); }",
        "2\n",
    );
}

#[test]
fn recursion() {
    assert_output(
        "int fib(int n) { if (n < 2) return n; return fib(n - 1) + fib(n - 2); }
         void main() { print(fib(20)); }",
        "6765\n",
    );
}

// Every call gets its own frame, so the caller's locals are left untouched.
#[test]
fn frames_are_separate() {
    assert_output(
        "int f(int x) { int y = x * 2; return y; }
         void main() { int y = 1; int x = f(5); print(x, \" \", y); }",
        "10 1\n",
    );
}

// Implicit `int` to `float` conversion on initialization, assignment,
// arguments, returns and mixed operations.
#[test]
fn int_to_float_conversions() {
    assert_output(
        "float half(float x) { return x / 2; }
         float one() { return 1; }
         void main() {
             float f = 3;
             print(f, \" \", half(1), \" \", one(), \" \", 7 / 2, \" \", 7 / 2.0);
             print(f = 2, \" \", 1 == 1.0, \" \", 2 > 1.5);
         }",
        "3.0 0.5 1.0 3 3.5\n2.0 true true\n",
    );
}

// Integer division truncates towards zero, as in C.
#[test]
fn integer_division_truncates() {
    assert_output(
        "void main() { print(7 / 2, \" \", -7 / 2, \" \", 7 / -2); }",
        "3 -3 -3\n",
    );
}

// Floats are printed in the shorter of decimal and scientific notation.
#[test]
fn float_formatting() {
    assert_output(
        "void main() { print(0.1, \" \", 1e20, \" \", 1.5e-7, \" \", 100.0, \" \", -0.0); }",
        "0.1 1e20 1.5e-7 100.0 -0.0\n",
    );
}

#[test]
fn float_division_by_zero_is_defined() {
    assert_output(
        "void main() { print(1 / 0.0, \" \", -1 / 0.0, \" \", 0 / 0.0); }",
        "inf -inf nan\n",
    );
}

// Ordering comparisons with NaN are always false.
#[test]
fn nan_comparisons() {
    assert_output(
        "void main() { float n = 0.0 / 0; print(n < 1, n > 1, n <= n, n >= n); }",
        "falsefalsefalsefalse\n",
    );
}

// A definition without initializer makes the variable uninitialized again,
// even when it held a value from a previous loop iteration.
#[test]
fn uninit_variable_in_loop() {
    assert_error(
        "void main() {
             for (int i = 0; i < 2; i = i + 1) {
                 int x;
                 if (i > 0) print(x);
                 x = i;
             }
         }",
        "attempted to read uninitialized variable `x`",
    );
}

#[test]
fn uninit_global() {
    assert_error(
        "int g; void main() { print(g); }",
        "attempted to read uninitialized variable `g`",
    );
    assert_output("int g; void main() { g = 1; print(g); }", "1\n");
}

// Assigning to an uninitialized variable doesn't read it.
#[test]
fn assign_to_uninit() {
    assert_output("void main() { int x; x = 1; print(x); }", "1\n");
}

#[test]
fn missing_return_on_some_path() {
    let program = "int sign(int x) { if (x > 0) return 1; if (x < 0) return -1; }
                   void main() { print(sign(5)); print(sign(0)); }";
    assert_error(program, "function `sign` with return type `int` did not return a value");
}

#[test]
fn void_function_without_return() {
    assert_output("void f() { print(1); } void main() { f(); f(); }", "1\n1\n");
}

#[test]
fn overflow_in_nested_call() {
    assert_error(
        "int square(int x) { return x * x; } void main() { print(square(3037000500)); }",
        "overflow computing `3037000500 * 3037000500`",
    );
}

#[test]
fn division_by_zero_via_variable() {
    assert_error(
        "void main() { int zero = 0; print(-5 / zero); }",
        "attempted to divide -5 by zero",
    );
}
