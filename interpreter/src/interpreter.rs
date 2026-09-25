//! The interpreter that executes C1 programs.
//!
//! This module contains the implementation of the interpreter that uses the
//! abstract syntax tree (AST) as well as the information calculated during
//! the semantic analysis to run a C1 program.
//!
//! It utilizes a virtual machine ([`VirtualMachine`]) helper to maintain the
//! state required for interpreting the AST, including managing global variables,
//! the function call stack (including local variables), and return values.
//!
//! The entry point for interpretation is the [`interpret`] function, which
//! initializes the interpreter, sets up global variables, and visits the main
//! function of the program.
//!
//! The module also defines the [`Interpreter`] structure, which provides
//! methods to visit and evaluate statements and expressions in the AST, handle
//! function calls, and manage control flow constructs such as loops and
//! conditionals.
//!
//! Additionally, the module defines the [`InterpretError`] structure for
//! communicating errors that occur during interpretation.

use std::fmt::{self, Write};

use crate::{analysis, ast};
use vm::{Value, Variable, VirtualMachine};

mod vm;

/// The entry function of the interpretation.
///
/// This function initializes the interpreter with the given AST and program
/// information, sets up the global variables, and visits the main function.
pub fn interpret(
    ast: &ast::Program,
    info: &analysis::ProgramInfo,
) -> Result<String, InterpretError> {
    let mut interpreter = Interpreter {
        ast,
        info,
        output: String::new(),
        vm: VirtualMachine::new(info.global_vars.len()),
    };

    // initialize the global variables in declaration order
    for &item_id in &info.global_vars {
        interpreter.visit_var_def(&ast[item_id])?;
    }

    // visit the main function
    let mut res_ident = ast::ResIdent::new(ast::Ident("main".to_owned()));
    res_ident.set_res(info.main_func.unwrap().0);
    interpreter.visit_func_call(&ast::FuncCall {
        res_ident,
        args: Vec::new(),
    })?;

    // return the resulting output
    Ok(interpreter.output)
}

/// Computes the *least upper bound* (LUB) of two types.
///
/// Given two types `a` and `b`, the LUB is a type `c`, such that
/// - both `a` and `b` can be cast to `c`, and
/// - `c` is the *most concrete* type that satisfies this property.
fn least_upper_bound(lhs: ast::DataType, rhs: ast::DataType) -> ast::DataType {
    use ast::DataType::*;
    match (lhs, rhs) {
        (a, b) if a == b => a,
        (Int, Float) | (Float, Int) => Float,
        (a, b) => unreachable!("invalid LUB of `{a:?}` and `{b:?}`"),
    }
}

/// Structure representing the interpreter.
///
/// This structure holds the state required for interpreting the AST, including
/// the AST, program information, output string, and the virtual machine storing
/// the variables.
#[derive(Debug, Clone, PartialEq)]
struct Interpreter<'a> {
    ast: &'a ast::Program,
    info: &'a analysis::ProgramInfo,
    output: String,
    vm: VirtualMachine,
}

impl Interpreter<'_> {
    /// Visits a statement.
    fn visit_stmt(&mut self, stmt: &ast::Stmt) -> Result<(), InterpretError> {
        use ast::Stmt::*;
        match stmt {
            Empty => Ok(()),
            If(inner) => self.visit_if_stmt(inner),
            For(inner) => self.visit_for_stmt(inner),
            While(inner) => self.visit_while_stmt(inner),
            DoWhile(inner) => self.visit_do_while_stmt(inner),
            Return(expr) => self.visit_return_stmt(expr),
            Print(inner) => self.visit_print_stmt(inner),
            VarDef(var_def) => self.visit_var_def(var_def),
            Assign(assign) => self.visit_assign(assign).map(|_| ()),
            Call(call) => self.visit_func_call(call).map(|_| ()),
            Block(block) => self.visit_block(block),
        }
    }

    /// Visits an expression and evaluates it.
    fn visit_expr(&mut self, expr: &ast::Expr) -> Result<Value, InterpretError> {
        use ast::Expr::*;
        match expr {
            BinaryOp(inner) => self.visit_bin_op_expr(inner),
            UnaryMinus(inner) => self.visit_unary_minus(inner),
            Assign(assign) => self.visit_assign(assign),
            Call(inner) => self.visit_func_call_expr(inner),
            Literal(literal) => Ok(literal.clone().into()),
            Var(res_ident) => self.visit_load_var(res_ident),
        }
    }

    /// Evaluates the condition of a control flow statement.
    fn visit_cond(&mut self, cond: &ast::Expr) -> Result<bool, InterpretError> {
        match self.visit_expr(cond)? {
            Value::Bool(value) => Ok(value),
            value => unreachable!("condition evaluated to non-bool value `{value:?}`"),
        }
    }

    /// Visits an `if` statement.
    fn visit_if_stmt(&mut self, stmt: &ast::IfStmt) -> Result<(), InterpretError> {
        if self.visit_cond(&stmt.cond)? {
            self.visit_stmt(&stmt.if_true)
        } else if let Some(if_false) = &stmt.if_false {
            self.visit_stmt(if_false)
        } else {
            Ok(())
        }
    }

    /// Visits a `for` statement.
    fn visit_for_stmt(&mut self, stmt: &ast::ForStmt) -> Result<(), InterpretError> {
        match &stmt.init {
            ast::ForInit::VarDef(var_def) => self.visit_var_def(var_def)?,
            ast::ForInit::Assign(assign) => {
                self.visit_assign(assign)?;
            }
        }

        while self.visit_cond(&stmt.cond)? {
            self.visit_stmt(&stmt.body)?;
            if self.vm.is_returning() {
                break;
            }
            self.visit_assign(&stmt.update)?;
        }

        Ok(())
    }

    /// Visits a `while` statement.
    fn visit_while_stmt(&mut self, stmt: &ast::WhileStmt) -> Result<(), InterpretError> {
        while self.visit_cond(&stmt.cond)? {
            self.visit_stmt(&stmt.body)?;
            if self.vm.is_returning() {
                break;
            }
        }

        Ok(())
    }

    /// Visits a `do-while` statement.
    fn visit_do_while_stmt(&mut self, stmt: &ast::WhileStmt) -> Result<(), InterpretError> {
        loop {
            self.visit_stmt(&stmt.body)?;
            if self.vm.is_returning() || !self.visit_cond(&stmt.cond)? {
                break;
            }
        }

        Ok(())
    }

    /// Visits a `return` statement, setting the return value.
    fn visit_return_stmt(&mut self, expr: &Option<ast::Expr>) -> Result<(), InterpretError> {
        let value = match expr {
            Some(expr) => Some(self.visit_expr(expr)?),
            None => None,
        };
        self.vm.set_return(value);
        Ok(())
    }

    /// Visits a `print` statement, writing into the output string.
    fn visit_print_stmt(&mut self, print: &ast::PrintStmt) -> Result<(), InterpretError> {
        for expr in &print.exprs {
            let value = self.visit_expr(expr)?;
            // We can `unwrap` here, because the standard library guarantees that
            // these can never return `Err`.
            match value {
                Value::String(value) => write!(self.output, "{value}"),
                Value::Int(value) => write!(self.output, "{value}"),
                Value::Bool(value) => write!(self.output, "{value}"),
                // Debug formatting picks the shorter of decimal and scientific
                // notation and prints infinities as `inf` and `-inf`.
                Value::Float(value) if value.is_nan() => write!(self.output, "nan"),
                Value::Float(value) => write!(self.output, "{value:?}"),
            }
            .unwrap();
        }
        writeln!(self.output).unwrap();
        Ok(())
    }

    /// Visits a variable definition and initializes it if possible.
    fn visit_var_def(&mut self, var_def: &ast::VarDef) -> Result<(), InterpretError> {
        let info = &self.info.definitions[var_def.res_ident.get_res()];
        match &var_def.init {
            Some(init) => {
                let value = self.visit_expr(init)?;
                self.vm.store_var(info, value);
            }
            // The definition may be executed repeatedly (e.g. in a loop), so the
            // variable must not keep a value from a previous execution.
            None => self.vm.reset_var(info),
        }
        Ok(())
    }

    /// Visits a block of statements and evaluates each one.
    fn visit_block(&mut self, block: &ast::Block) -> Result<(), InterpretError> {
        for stmt in &block.statements {
            self.visit_stmt(stmt)?;
            if self.vm.is_returning() {
                break;
            }
        }
        Ok(())
    }

    /// Visits a function call and evaluates it.
    fn visit_func_call(&mut self, call: &ast::FuncCall) -> Result<Variable, InterpretError> {
        let def_id = call.res_ident.get_res();
        let func_info = match &self.info.definitions[def_id] {
            analysis::DefInfo::Func(info) => info,
            _ => unreachable!("function call resolves to non-function"),
        };
        let func_def = &self.ast[func_info.item_id];

        // Evaluate the arguments from left to right in the caller's frame.
        let mut args = Vec::with_capacity(call.args.len());
        for arg in &call.args {
            args.push(self.visit_expr(arg)?);
        }

        // Set up a new frame with all local variables uninitialized and
        // initialize the parameters with the argument values.
        self.vm
            .push_frame(vec![Variable::Uninit; func_info.local_vars.len()]);
        for (&param, arg) in func_info.params().iter().zip(args) {
            self.vm.store_var(&self.info.definitions[param.def_id()], arg);
        }

        // Evaluate the function body.
        for stmt in &func_def.statements {
            self.visit_stmt(stmt)?;
            if self.vm.is_returning() {
                break;
            }
        }

        let return_var = self.vm.take_return();
        self.vm.pop_frame();

        match return_var {
            Variable::Init(value) => Ok(Variable::Init(value.cast(func_info.return_type))),
            Variable::Uninit if func_info.return_type == ast::DataType::Void => {
                Ok(Variable::Uninit)
            }
            // Reaching the end of a non-void function is undefined behavior,
            // even if the result is discarded.
            Variable::Uninit => Err(InterpretError(format!(
                "function {} with return type `{}` did not return a value",
                func_def.ident, func_info.return_type,
            ))),
        }
    }

    /// Visits a binary operation expression and evaluates it.
    fn visit_bin_op_expr(
        &mut self,
        bin_op_expr: &ast::BinOpExpr,
    ) -> Result<Value, InterpretError> {
        use ast::BinOp;

        let op = bin_op_expr.op;
        let lhs = self.visit_expr(&bin_op_expr.lhs)?;

        // As in C, logical operators only evaluate the right operand if the
        // result isn't determined by the left one yet.
        if let BinOp::LogAnd | BinOp::LogOr = op {
            let Value::Bool(lhs) = lhs else {
                unreachable!("logical operator applied to `{lhs:?}`");
            };
            if lhs == (op == BinOp::LogOr) {
                return Ok(Value::Bool(lhs));
            }
            return match self.visit_expr(&bin_op_expr.rhs)? {
                Value::Bool(rhs) => Ok(Value::Bool(rhs)),
                rhs => unreachable!("logical operator applied to `{rhs:?}`"),
            };
        }

        let rhs = self.visit_expr(&bin_op_expr.rhs)?;

        // Convert both operands to their common type, e.g. `1 + 2.5` to `1.0 + 2.5`.
        let common_type = least_upper_bound(lhs.data_type(), rhs.data_type());
        let (lhs, rhs) = (lhs.cast(common_type), rhs.cast(common_type));

        let result = match op {
            BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div => match (lhs, rhs) {
                (Value::Int(lhs), Value::Int(rhs)) => Value::Int(Self::int_arith(op, lhs, rhs)?),
                (Value::Float(lhs), Value::Float(rhs)) => Value::Float(match op {
                    BinOp::Add => lhs + rhs,
                    BinOp::Sub => lhs - rhs,
                    BinOp::Mul => lhs * rhs,
                    _ => lhs / rhs,
                }),
                (lhs, rhs) => unreachable!("arithmetic on `{lhs:?}` and `{rhs:?}`"),
            },
            BinOp::Eq => Value::Bool(lhs == rhs),
            BinOp::Neq => Value::Bool(lhs != rhs),
            BinOp::Lt | BinOp::Gt | BinOp::Leq | BinOp::Geq => {
                // `partial_cmp` returns `None` for NaN, which makes every
                // ordering comparison false, as in C.
                let ordering = match (&lhs, &rhs) {
                    (Value::Int(lhs), Value::Int(rhs)) => lhs.partial_cmp(rhs),
                    (Value::Float(lhs), Value::Float(rhs)) => lhs.partial_cmp(rhs),
                    (Value::Bool(lhs), Value::Bool(rhs)) => lhs.partial_cmp(rhs),
                    (Value::String(lhs), Value::String(rhs)) => lhs.partial_cmp(rhs),
                    (lhs, rhs) => unreachable!("comparison of `{lhs:?}` and `{rhs:?}`"),
                };
                Value::Bool(ordering.is_some_and(|ordering| match op {
                    BinOp::Lt => ordering.is_lt(),
                    BinOp::Gt => ordering.is_gt(),
                    BinOp::Leq => ordering.is_le(),
                    _ => ordering.is_ge(),
                }))
            }
            BinOp::LogAnd | BinOp::LogOr => unreachable!("handled above"),
        };

        Ok(result)
    }

    /// Performs integer arithmetic, reporting overflow and division by zero.
    fn int_arith(op: ast::BinOp, lhs: i64, rhs: i64) -> Result<i64, InterpretError> {
        let result = match op {
            ast::BinOp::Add => lhs.checked_add(rhs),
            ast::BinOp::Sub => lhs.checked_sub(rhs),
            ast::BinOp::Mul => lhs.checked_mul(rhs),
            ast::BinOp::Div => {
                if rhs == 0 {
                    return Err(InterpretError(format!(
                        "attempted to divide {lhs} by zero"
                    )));
                }
                lhs.checked_div(rhs)
            }
            _ => unreachable!("`{op}` is no arithmetic operator"),
        };

        result.ok_or_else(|| InterpretError(format!("overflow computing `{lhs} {op} {rhs}`")))
    }

    /// Visits a unary minus expression and evaluates it.
    fn visit_unary_minus(&mut self, inner_expr: &ast::Expr) -> Result<Value, InterpretError> {
        match self.visit_expr(inner_expr)? {
            Value::Int(value) => value
                .checked_neg()
                .map(Value::Int)
                .ok_or_else(|| InterpretError("overflow negating minimum int".to_owned())),
            Value::Float(value) => Ok(Value::Float(-value)),
            value => unreachable!("unary minus applied to `{value:?}`"),
        }
    }

    /// Visits an assignment expression and evaluates it.
    fn visit_assign(&mut self, assign: &ast::Assign) -> Result<Value, InterpretError> {
        let value = self.visit_expr(&assign.rhs)?;
        let info = &self.info.definitions[assign.lhs.get_res()];
        // The stored value is cast to the variable type, which is also the
        // type of the assignment expression.
        Ok(self.vm.store_var(info, value))
    }

    /// Visits a function call expression and evaluates it.
    fn visit_func_call_expr(&mut self, call: &ast::FuncCall) -> Result<Value, InterpretError> {
        match self.visit_func_call(call)? {
            Variable::Init(value) => Ok(value),
            Variable::Uninit => unreachable!("value of void function {} used", call.res_ident),
        }
    }

    /// Visits a variable and loads its value.
    fn visit_load_var(&mut self, ident: &ast::ResIdent) -> Result<Value, InterpretError> {
        match self.vm.load_var(&self.info.definitions[ident.get_res()]) {
            Variable::Init(value) => Ok(value),
            Variable::Uninit => Err(InterpretError(format!(
                "attempted to read uninitialized variable {ident}"
            ))),
        }
    }
}

/// Structure representing an interpretation error.
///
/// This structure holds a human-readable error message string.
#[derive(Debug, Clone, PartialEq)]
pub struct InterpretError(String);

impl fmt::Display for InterpretError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
