mod util;

mod cases {
    include!(concat!(env!("OUT_DIR"), "/cases_interpreter.rs"));
}

fn run_test_ok(path: &str) {
    util::run_test_ok(path, "output", |input| {
        let mut ast = c1::parse(input).expect("parsing failed");
        let analysis = c1::analyze(&mut ast).expect("analysis failed");
        c1::interpret(&ast, &analysis).expect("interpreting failed")
    });
}

fn run_test_err(path: &str) {
    util::run_test_err(path, |input| {
        let mut ast = c1::parse(input).expect("parsing failed");
        let analysis = c1::analyze(&mut ast).expect("analysis failed");
        let err = c1::interpret(&ast, &analysis).expect_err("interpreting passed");
        format!("{err}")
    });
}
