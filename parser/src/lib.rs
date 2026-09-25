#![forbid(unsafe_code)]

pub mod ast;
pub mod lexer;

pub use ast::parse;
