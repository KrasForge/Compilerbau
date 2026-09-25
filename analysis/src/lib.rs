#![forbid(unsafe_code)]

pub mod analysis;
pub mod ast;
pub mod lexer;

pub use analysis::analyze;
pub use ast::parse;
