use std::{env, fs, process::ExitCode};

use c1::parse;

fn main() -> ExitCode {
    let mut args = env::args();

    if args.len() < 2 {
        println!(
            "Usage: {} <c1-source>",
            args.next().as_deref().unwrap_or("c1")
        );
        return ExitCode::FAILURE;
    }

    // Read the source file into a string.
    let input = match fs::read_to_string(args.nth(1).unwrap()) {
        Ok(input) => input,
        Err(err) => {
            println!("failed to read c1 source file: {err}");
            return ExitCode::FAILURE;
        }
    };

    // Parse the source code into an AST.
    let ast = match parse(&input) {
        Ok(ast) => {
            println!("[✓] syntax");
            ast
        }
        Err(err) => {
            println!("[x] syntax");
            println!("{err}");
            return ExitCode::FAILURE;
        }
    };

    println!("{ast:#?}");

    ExitCode::SUCCESS
}
