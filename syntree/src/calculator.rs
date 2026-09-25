//! This module provides the [`Calculator`] struct, which is designed to
//! evaluate arithmetic expressions parsed into a [`Root`] structure from a
//! string representation. It supports operations including addition,
//! subtraction, multiplication, and division, along with variable assignments
//! from 'a' to 'z'.
//!
//! ## Example
//! ```
//! # use syntree::{Root, Visitor, Calculator};
//!
//! let mut calculator = Calculator::default();
//! let root = Root::from_str("a 1 = b 2 3 * = a b +").unwrap();
//! let result = calculator.calc(&root);
//!
//! println!("Final result: {}", result); // prints 7
//! ```

use crate::parse_tree::*;

/// `Calculator` is a struct designed to evaluate parsed arithmetic expressions.
///
/// ## Usage
/// ```
/// # use syntree::{Calculator, Root};
/// # fn doc(root: Root) {
/// let mut calculator = Calculator::default();
/// let result = calculator.calc(&root);
/// println!("The result of the calculation is: {}", result);
/// # }
/// ```
#[derive(Default)]
pub struct Calculator {
	/// Values of the variables 'a' to 'z'; unassigned variables read as 0.
	vars: [i64; 26],
	/// Operand stack holding intermediate results during evaluation.
	stack: Vec<i64>,
	/// Value of the most recently evaluated statement.
	result: i64,
}

impl Calculator {
	/// Evaluates the entire parse tree starting from a [`Root`] and returns the
	/// result of the last expression evaluated.
	pub fn calc(&mut self, t: &Root) -> i64 {
		self.visit_root(t);
		self.result
	}
	
	/// Maps a variable name to its slot in `vars`.
	fn slot(name: char) -> usize {
		assert!(name.is_ascii_lowercase(), "invalid variable name {name:?}");
		(name as u8 - b'a') as usize
	}
	
	/// Evaluates a binary operation on the results of both operands.
	fn binary(&mut self, lhs: &Expr, rhs: &Expr, op: fn(i64, i64) -> i64) {
		self.visit_expr(lhs);
		self.visit_expr(rhs);
		let r = self.pop();
		let l = self.pop();
		self.stack.push(op(l, r));
	}
	
	fn pop(&mut self) -> i64 {
		self.stack.pop().expect("operand stack underflow")
	}
}

impl Visitor for Calculator {
	fn visit_stmt(&mut self, s: &Stmt) {
		match s {
			Stmt::Expr(e) => {
				self.visit_expr(e);
				self.result = self.pop();
			}
			Stmt::Set(name, e) => {
				self.visit_expr(e);
				self.result = self.pop();
				self.vars[Self::slot(*name)] = self.result;
			}
		}
	}
	
	fn visit_expr(&mut self, e: &Expr) {
		match e {
			Expr::Int(n) => self.stack.push(*n),
			Expr::Var(name) => self.stack.push(self.vars[Self::slot(*name)]),
			Expr::Add(lhs, rhs) => self.binary(lhs, rhs, |l, r| l + r),
			Expr::Sub(lhs, rhs) => self.binary(lhs, rhs, |l, r| l - r),
			Expr::Mul(lhs, rhs) => self.binary(lhs, rhs, |l, r| l * r),
			Expr::Div(lhs, rhs) => self.binary(lhs, rhs, |l, r| l / r),
		}
	}
}

// unit-tests

#[cfg(test)]
mod tests {
	use super::*;
	
	#[test]
	fn add() {
		let tree = Root::from_stmt(Stmt::add(4, 2));
		assert_eq!(Calculator::default().calc(&tree), 6);
	}
	
	#[test]
	fn sub() {
		let tree = Root::from_stmt(Stmt::sub(4, 2));
		assert_eq!(Calculator::default().calc(&tree), 2);
	}
	
	#[test]
	fn mul() {
		let tree = Root::from_stmt(Stmt::mul(4, 2));
		assert_eq!(Calculator::default().calc(&tree), 8);
	}
	
	#[test]
	fn div() {
		let tree = Root::from_stmt(Stmt::div(4, 2));
		assert_eq!(Calculator::default().calc(&tree), 2);
	}
	
	#[test]
	#[should_panic(expected = "attempt to divide by zero")]
	fn division_by_zero() {
		let tree = Root::from_stmt(Stmt::div(4, 0));
		Calculator::default().calc(&tree);
	}
	
	#[test]
	fn set() {
		let tree = Root {
			stmt_list: vec![
				Stmt::set('a', 1),
				Stmt::Expr(Expr::Var('a'))
			]
		};
		assert_eq!(Calculator::default().calc(&tree), 1);
	}
	
	#[test]
	fn vars() {
		let tree = Root {
			stmt_list: vec![
				Stmt::set('i', 1),
				Stmt::set('j', 2),
				Stmt::Expr(Expr::Add(
					Box::new(Expr::Var('i')),
					Box::new(Expr::Var('j')),
				)),
			],
		};
		assert_eq!(Calculator::default().calc(&tree), 3);
	}
}
