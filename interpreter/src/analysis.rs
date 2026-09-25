//! The semantic analysis pass of C1.
//!
//! # Overview
//!
//! The entry point of the semantic analysis pass is the [`analyze`] function.
//! It is called on the AST root ([`ast::Program`]) after parsing and has the
//! following three goals:
//!
//! - Checking the static semantic of the program, i.e. performing all "compile-time"
//!   checks. For example, this includes checking whether the operands of an addition
//!   like `a + b` are compatible. If a static semantic rule is violated, [`analyze`]
//!   returns a human-readable error message in [`AnalysisError`].
//!
//! - Collecting auxiliary information about the definition of functions and variables
//!   that will be used by the interpreter. For example, this includes the size of the
//!   stack frame for every function, i.e. how many local variables it has. If no error
//!   occurs, [`analyze`] returns this information as [`ProgramInfo`].
//!
//! - Resolving all resolvable identifiers ([`ast::ResIdent`]s) to a definition. The
//!   resolutions are written directly into the AST with [`ast::ResIdent::set_res`].
//!
//! The central data type that powers our semantic analysis pass is the [`Analyzer`],
//! which uses its `visit_*` methods to traverses the AST in a depth-first manner,
//! similar to the [visitor pattern].
//!
//! [visitor pattern]: https://en.wikipedia.org/wiki/Visitor_pattern
//!
//! # Static semantic
//!
//! The full semantic rules are provided in a separate document. The analyzer checks
//! the rules in the `visit_*` methods and bubbles out the first error that it encounters
//! via the try (`?`) operator. We do not attempt error recovery.
//!
//! # Auxiliary information
//!
//! The auxiliary information of all definitions is stored a single vector in
//! [`Definitions`]. The [`ast::DefId`] that is stored directly in the AST as part of
//! the [`ast::ResIdent`]s represents an index into this vector of definitions.
//!
//! The [`FuncInfo`] and [`VarInfo`] contain the auxiliary information required
//! by the interpreter about function and variable definitions respectively. When stored
//! in the [`Definitions`], they are combined in the enum [`DefInfo`].
//!
//! The top-level auxiliary data structure produced by the [`analyze`] function is
//! [`ProgramInfo`], which contains the [`Definitions`] and some additional
//! information about the main function and global variables.
//!
//! # Name resolution
//!
//! The analyzer uses a block-structured symbol table ([`Symtab`]) to do name
//! resolution.
//!
//! Scopes are crated and deleted with the [`Symtab::scope_enter`] and [`Symtab::scope_leave`]
//! functions. New symbols in the innermost scope are created with the `Symtab::define_*` family
//! of methods and symbols are resolved with the [`Symtab::resolve`] method.
//!
//! Calling `resolve` returns a `DefId`, which can be used as index into [`Analyzer`]
//! to retrieve additional information about the definition.
//!
//! # Examples
//!
//! This example demonstrates how to use the public API of this module: We use the auxiliary
//! information and resolved names produced by the semantic analysis to query some information
//! about our program:
//!
//! ```
//! use c1::{ast, analysis};
//!
//! let input = "void main() {
//!     int answer = 42;
//!     print(answer);
//! }";
//!
//! let mut ast = ast::parse(input).unwrap();
//! let analysis = analysis::analyze(&mut ast).unwrap();
//!
//! // How many local variables does main have?
//! let main_func_analysis = &analysis[analysis.main_func.unwrap()];
//! let main_locals = main_func_analysis.local_vars.len();
//! assert_eq!(main_locals, 1);
//!
//! // What definition did `answer` in `print(answer)` resolve to?
//! let main_func_item = &ast[main_func_analysis.item_id];
//! let ast::Stmt::Print(ast::PrintStmt { exprs: print_stmt }) = &main_func_item.statements[1] else { unreachable!() };
//! let ast::Expr::Var(answer_res) = &print_stmt[0] else { unreachable!() };
//! let answer_def_id = answer_res.get_res();
//! assert_eq!(answer_def_id, ast::DefId(1));
//!
//! // What memory location should `answer` in `print(answer)` be loaded from?
//! let answer_analysis = &analysis.definitions[answer_def_id];
//! // (The real interpreter should also handle `analysis::DefAnalysis::GlobalVar` here.)
//! let analysis::DefInfo::LocalVar(var_analysis) = answer_analysis else { unreachable!() };
//! assert_eq!(var_analysis.offset, 0);
//! ```
//!
//! Passing in an invalid program will produce an error:
//!
//! ```
//! use c1::{ast, analysis};
//!
//! let input = "void main() {
//!     int x = true;
//! }";
//!
//! let mut ast = ast::parse(input).unwrap();
//! let error = analysis::analyze(&mut ast).unwrap_err();
//! println!("{error}");
//! ```

use std::ops::Index;
use std::fmt;

pub use symtab::{DefInfo, Definitions, FuncDefId, FuncInfo, LocalVarDefId, Symtab, VarInfo};

use crate::ast;

mod symtab;

/// The entry function of the semantic analysis pass.
///
/// This function supports forward referencing of functions: a pre-pass
/// registers all function definitions before the main analysis pass, so
/// functions can be called before they are defined.
pub fn analyze(root: &mut ast::Program) -> Result<ProgramInfo, AnalysisError> {
    let mut analyzer = Analyzer::default();

    // Pre-pass: register all function definitions.
    for (index, item) in root.items.iter().enumerate() {
        if let ast::Item::Func(func_def) = item {
            let item_id = ast::ItemId(index);
            analyzer.tab.define_func(
                func_def.ident.clone(),
                func_def.return_type,
                &func_def.params,
                ast::FuncItemId(item_id),
            )?;
        }
    }

    // Main pass: full semantic analysis.
    for (index, item) in root.items.iter_mut().enumerate() {
        let item_id = ast::ItemId(index);
        analyzer.visit_item(item_id, item)?;
    }

    let main_func = Some(analyzer.check_main_func()?);
    Ok(analyzer.tab.into_program_info(main_func))
}

/// The visitor that drives the semantic analysis pass.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Analyzer {
    /// The block-structured symbol table.
    tab: Symtab,
}

impl Analyzer {
    /// Analyzes an item.
    fn visit_item(
        &mut self,
        item_id: ast::ItemId,
        item: &mut ast::Item,
    ) -> Result<(), AnalysisError> {
        match item {
            ast::Item::GlobalVar(var_def) => {
                self.visit_global_var_def(ast::GlobalVarItemId(item_id), var_def)
            }
            ast::Item::Func(func_def) => self.visit_func_def(func_def),
        }
    }

    /// Analyzes a global variable definition.
    fn visit_global_var_def(
        &mut self,
        item_id: ast::GlobalVarItemId,
        var_def: &mut ast::VarDef,
    ) -> Result<(), AnalysisError> {
        // We check the initializer before defining the variable, so that
        // `int x = x` resolves the initializer to a previous definition.
        self.visit_var_def(var_def, "global variable")?;

        let def_id = self
            .tab
            .define_global_var(&var_def.res_ident, var_def.data_type, item_id)?;
        var_def.res_ident.set_res(def_id);

        Ok(())
    }

    /// Analyzes a local variable definition (not a function parameter).
    fn visit_local_var_def(&mut self, var_def: &mut ast::VarDef) -> Result<(), AnalysisError> {
        // We check the initializer before defining the variable, so that
        // `int x = x` resolves the initializer to a previous definition.
        self.visit_var_def(var_def, "local variable")?;

        let def_id = self
            .tab
            .define_local_var(&var_def.res_ident, var_def.data_type)?;
        var_def.res_ident.set_res(def_id);

        Ok(())
    }

    /// Analyzes a variable definition.
    ///
    /// This method contains code that is shared between [`Self::visit_global_var_def`] and
    /// [`Self::visit_local_var_def`].
    ///
    /// The `kind` parameter can be used to distinguish global and local vars in diagnostics.
    fn visit_var_def(
        &mut self,
        var_def: &mut ast::VarDef,
        kind: &str,
    ) -> Result<(), AnalysisError> {
        if var_def.data_type == ast::DataType::Void {
            let msg = format!(
                "cannot define {kind} {} with type `void`",
                var_def.res_ident
            );
            return Err(AnalysisError(msg));
        }

        let Some(init) = &mut var_def.init else {
            return Ok(());
        };

        let init_type = self.visit_expr(init)?;
        if Self::check_cast(init_type, var_def.data_type).is_err() {
            let msg = format!(
                "cannot initialize variable {} of type `{}` with value of type `{}`",
                var_def.res_ident, var_def.data_type, init_type,
            );
            return Err(AnalysisError(msg));
        }
        Ok(())
    }

    /// Analyzes a function definition.
    fn visit_func_def(
        &mut self,
        func_def: &mut ast::FuncDef,
    ) -> Result<(), AnalysisError> {
        // The function was already defined in the pre-pass.
        // Resolve it and set it as the current function.
        let def_id = self
            .tab
            .resolve(&func_def.ident)
            .unwrap_or_else(|_| panic!("function {} should have been defined in pre-pass", func_def.ident));
        self.tab.set_current_func(FuncDefId(def_id));

        self.tab.scope_enter();

        // ...but the params are defined in the inner scope.
        for param in &func_def.params {
            self.visit_func_param(param)?;
        }

        for stmt in &mut func_def.statements {
            self.visit_stmt(stmt)?;
        }

        self.tab.scope_leave();
        Ok(())
    }

    /// Analyzes a function parameter.
    fn visit_func_param(&mut self, param: &ast::FuncParam) -> Result<(), AnalysisError> {
        if param.data_type == ast::DataType::Void {
            let msg = format!(
                "cannot define function parameter {} with type `void`",
                param.ident
            );
            return Err(AnalysisError(msg));
        }
        
        self.tab.define_local_var(&param.ident, param.data_type)?;

        Ok(())
    }

    /// Analyzes a statement.
    fn visit_stmt(&mut self, stmt: &mut ast::Stmt) -> Result<(), AnalysisError> {
        match stmt {
            ast::Stmt::Empty => Ok(()),
            ast::Stmt::If(inner) => self.visit_if_stmt(inner),
            ast::Stmt::For(inner) => self.visit_for_stmt(inner),
            ast::Stmt::While(inner) => self.visit_while_stmt(inner, "while loop"),
            ast::Stmt::DoWhile(inner) => self.visit_while_stmt(inner, "do-while loop"),
            ast::Stmt::Return(expr) => self.visit_return_stmt(expr),
            ast::Stmt::Print(inner) => self.visit_print_stmt(inner),
            ast::Stmt::VarDef(var_def) => self.visit_local_var_def(var_def),
            ast::Stmt::Assign(assign) => {
                // Ignore the expression type.
                self.visit_assign(assign)?;
                Ok(())
            }
            ast::Stmt::Call(call) => {
                // Ignore the return type.
                self.visit_call(call)?;
                Ok(())
            }
            ast::Stmt::Block(block) => self.visit_block(block),
        }
    }

    /// Analyzes a block statement.
    fn visit_block(&mut self, block: &mut ast::Block) -> Result<(), AnalysisError> {
        self.tab.scope_enter();
        for stmt in &mut block.statements {
            self.visit_stmt(stmt)?;
        }
        self.tab.scope_leave();
        Ok(())
    }

    /// Analyzes an `if` statement.
    fn visit_if_stmt(&mut self, stmt: &mut ast::IfStmt) -> Result<(), AnalysisError> {
        self.visit_cond_expr(&mut stmt.cond, "if")?;
        self.visit_stmt(&mut stmt.if_true)?;
        if let Some(if_false) = &mut stmt.if_false {
            self.visit_stmt(if_false)?;
        }
        Ok(())
    }

    /// Analyzes a `for` statement.
    fn visit_for_stmt(&mut self, stmt: &mut ast::ForStmt) -> Result<(), AnalysisError> {
        self.tab.scope_enter();

        match &mut stmt.init {
            ast::ForInit::VarDef(var_def) => self.visit_local_var_def(var_def)?,
            ast::ForInit::Assign(assign) => {
                // Ignore the expression type.
                self.visit_assign(assign)?;
            }
        }

        self.visit_cond_expr(&mut stmt.cond, "for loop")?;
        self.visit_assign(&mut stmt.update)?;
        self.visit_stmt(&mut stmt.body)?;

        self.tab.scope_leave();
        Ok(())
    }

    /// Analyzes a `while` or `do`-`while` statement.
    ///
    /// The `kind` parameter can be used to distinguish them in diagnostics.
    fn visit_while_stmt(
        &mut self,
        stmt: &mut ast::WhileStmt,
        kind: &str,
    ) -> Result<(), AnalysisError> {
        self.visit_cond_expr(&mut stmt.cond, kind)?;
        self.visit_stmt(&mut stmt.body)
    }

    /// Analyzes a `return` statement.
    fn visit_return_stmt(&mut self, expr: &mut Option<ast::Expr>) -> Result<(), AnalysisError> {
        let func_return_type = self
            .tab
            .current_func()
            .expect("expected to be nested in function")
            .return_type;

        if let Some(expr) = expr {
            if func_return_type == ast::DataType::Void {
                let msg = "cannot `return` with value in function returning `void`".to_owned();
                return Err(AnalysisError(msg));
            }

            let expr_type = self.visit_expr(expr)?;
            if Self::check_cast(expr_type, func_return_type).is_err() {
                let msg = format!(
                    "cannot return value of type `{expr_type}` from function \
                    returning `{func_return_type}`",
                );
                return Err(AnalysisError(msg));
            }
            Ok(())
        } else if func_return_type != ast::DataType::Void {
            let msg = format!(
                "cannot `return;` without value in function returning `{func_return_type}`"
            );
            Err(AnalysisError(msg))
        } else {
            Ok(())
        }
    }

    /// Analyzes a `print` statement.
    fn visit_print_stmt(&mut self, print: &mut ast::PrintStmt) -> Result<(), AnalysisError> {
        for expr in &mut print.exprs {
            let expr_type = self.visit_expr(expr)?;
            if expr_type == ast::DataType::Void {
                let msg = "cannot `print` value of type `void`".to_owned();
                return Err(AnalysisError(msg));
            }
        }
        
        Ok(())
    }

    /// Analyzes a call statement or expression and returns its return type.
    fn visit_call(&mut self, call: &mut ast::FuncCall) -> Result<ast::DataType, AnalysisError> {
        let def_id = self.tab.resolve(&call.res_ident)?;
        call.res_ident.set_res(def_id);

        let ident = &call.res_ident;
        let DefInfo::Func(func_analysis) = &self.tab[def_id] else {
            let msg = format!("cannot call variable {ident}");
            return Err(AnalysisError(msg));
        };

        // Check the argument count first.
        if func_analysis.param_count() != call.args.len() {
            let msg = format!(
                "incorrect number of arguments in call to {ident}, expected {}, found {}",
                func_analysis.param_count(),
                call.args.len(),
            );
            return Err(AnalysisError(msg));
        }

        // Read the parameter types directly from the function signature.
        let param_types = func_analysis.param_types.clone();
        let return_type = func_analysis.return_type;

        // Compare the types of the arguments (provided) with the types of the
        // parameters (expected).
        for (index, (param_type, arg_expr)) in std::iter::zip(param_types, &mut call.args).enumerate() {
            let arg_type = self.visit_expr(arg_expr)?;
            if Self::check_cast(arg_type, param_type).is_err() {
                let msg = format!(
                    "incorrect type for argument {index} in call to {ident}, \
                    expected `{param_type}`, found `{arg_type}`",
                );
                return Err(AnalysisError(msg));
            }
        }

        Ok(return_type)
    }

    /// Analyzes an assignment statement or expression and returns its type.
    fn visit_assign(&mut self, assign: &mut ast::Assign) -> Result<ast::DataType, AnalysisError> {
        let lhs_def_id = self.tab.resolve(&assign.lhs)?;
        assign.lhs.set_res(lhs_def_id);

        let lhs_type = match &self.tab[lhs_def_id] {
            DefInfo::Func(_) => {
                let msg = format!("cannot assign to function {}", assign.lhs);
                return Err(AnalysisError(msg));
            }
            DefInfo::GlobalVar(analysis) => analysis.data_type,
            DefInfo::LocalVar(analysis) => analysis.data_type,
        };

        let rhs_type = self.visit_expr(&mut assign.rhs)?;
        if Self::check_cast(rhs_type, lhs_type).is_err() {
            let msg = format!(
                "cannot assign value of type `{rhs_type}` to variable {} of type `{lhs_type}`",
                assign.lhs,
            );
            return Err(AnalysisError(msg));
        }

        Ok(lhs_type)
    }

    /// Analyzes the condition expression of a control flow statement, expecting
    /// a boolean type.
    ///
    /// The `kind` parameter describes the statement for diagnostics.
    fn visit_cond_expr(&mut self, expr: &mut ast::Expr, kind: &str) -> Result<(), AnalysisError> {
        let cond_type = self.visit_expr(expr)?;
        if cond_type != ast::DataType::Bool {
            let msg = format!("condition of {kind} must have type `bool`, found `{cond_type}`");
            return Err(AnalysisError(msg));
        }
        Ok(())
    }

    /// Analyzes an expression and returns its type.
    fn visit_expr(&mut self, expr: &mut ast::Expr) -> Result<ast::DataType, AnalysisError> {
        match expr {
            ast::Expr::BinaryOp(inner) => self.visit_bin_op_expr(inner),
            ast::Expr::UnaryMinus(inner) => self.visit_unary_minus_expr(inner),
            ast::Expr::Assign(inner) => self.visit_assign(inner),
            ast::Expr::Call(inner) => self.visit_call(inner),
            ast::Expr::Literal(literal) => Ok(literal.data_type()),
            ast::Expr::Var(res_ident) => self.visit_var_expr(res_ident),
        }
    }

    /// Analyzes a binary operator expression and returns its type.
    fn visit_bin_op_expr(
        &mut self,
        bin_op_expr: &mut ast::BinOpExpr,
    ) -> Result<ast::DataType, AnalysisError> {
        let lhs_type = self.visit_expr(&mut bin_op_expr.lhs)?;
        let rhs_type = self.visit_expr(&mut bin_op_expr.rhs)?;

        let Ok(lub_type) = Self::check_least_upper_bound(lhs_type, rhs_type) else {
            let msg = format!(
                "cannot apply binary operator to incompatible types: `{} {} {}`",
                lhs_type, bin_op_expr.op, rhs_type,
            );
            return Err(AnalysisError(msg));
        };

        match bin_op_expr.op {
            ast::BinOp::Add | ast::BinOp::Sub | ast::BinOp::Mul | ast::BinOp::Div => {
                if !lub_type.is_numeric() {
                    let msg = format!(
                        "cannot use arithmetic operator `{}` with values of type `{}`",
                        bin_op_expr.op, lub_type,
                    );
                    return Err(AnalysisError(msg));
                }
                Ok(lub_type)
            }
            ast::BinOp::LogOr | ast::BinOp::LogAnd => {
                if lub_type != ast::DataType::Bool {
                    let msg = format!(
                        "cannot use logical operator `{}` with values of type `{}`",
                        bin_op_expr.op, lub_type,
                    );
                    return Err(AnalysisError(msg));
                }
                Ok(ast::DataType::Bool)
            }
            ast::BinOp::Eq
            | ast::BinOp::Neq
            | ast::BinOp::Lt
            | ast::BinOp::Gt
            | ast::BinOp::Leq
            | ast::BinOp::Geq => {
                if lub_type == ast::DataType::Void {
                    let msg = format!(
                        "cannot use comparison operator `{}` with values of type `{}`",
                        bin_op_expr.op, lub_type,
                    );
                    return Err(AnalysisError(msg));
                }
                Ok(ast::DataType::Bool)
            }
        }
    }

    /// Analyzes an unary minus expression and returns its type.
    fn visit_unary_minus_expr(
        &mut self,
        inner_expr: &mut ast::Expr,
    ) -> Result<ast::DataType, AnalysisError> {
        let inner_type = self.visit_expr(inner_expr)?;
        if !inner_type.is_numeric() {
            let msg = format!("cannot apply unary minus to type `{inner_type}`");
            return Err(AnalysisError(msg));
        }
        Ok(inner_type)
    }

    /// Analyzes a variable expression and returns its type.
    fn visit_var_expr(
        &mut self,
        res_ident: &mut ast::ResIdent,
    ) -> Result<ast::DataType, AnalysisError> {
        let def_id = self.tab.resolve(res_ident)?;
        res_ident.set_res(def_id);
        match &self.tab[def_id] {
            DefInfo::Func(_) => {
                let msg = format!("cannot load function {} as a value", res_ident);
                Err(AnalysisError(msg))
            }
            DefInfo::GlobalVar(analysis) => Ok(analysis.data_type),
            DefInfo::LocalVar(analysis) => Ok(analysis.data_type),
        }
    }

    /// Checks whether `src_type` is compatible with `dst_type`.
    ///
    /// This method returns does not return an [`AnalysisError`] directly, because
    /// it's caller has more context to give a better error message.
    fn check_cast(src_type: ast::DataType, dst_type: ast::DataType) -> Result<(), ()> {
        match (src_type, dst_type) {
            (src, dst) if src == dst => Ok(()),
            (ast::DataType::Int, ast::DataType::Float) => Ok(()),
            (_, _) => Err(()),
        }
    }

    /// Computes the *least upper bound* (LUB) of two types.
    ///
    /// Given two types `a` and `b`, the LUB is a type `c`, such that
    /// - both `a` and `b` can be cast to `c`, and
    /// - `c` is the *most concrete* type that satisfies this property, i.e.
    ///   there is no type `d` such that `d` can be cast to `c`, and `a` and
    ///   `b` can be cast to `d`.
    ///
    /// This method returns does not return an [`AnalysisError`] directly, because
    /// it's caller has more context to give a better error message.
    fn check_least_upper_bound(a: ast::DataType, b: ast::DataType) -> Result<ast::DataType, ()> {
        match (a, b) {
            (a, b) if a == b => Ok(a),
            (ast::DataType::Int, ast::DataType::Float)
            | (ast::DataType::Float, ast::DataType::Int) => Ok(ast::DataType::Float),
            (_, _) => Err(()),
        }
    }

    /// Resolves the main function, checks its semantic rules, and returns its definition id.
    fn check_main_func(&self) -> Result<FuncDefId, AnalysisError> {
        let Ok(main_def_id) = self.tab.resolve("main") else {
            let msg = "cannot find main function".to_owned();
            return Err(AnalysisError(msg));
        };

        let DefInfo::Func(main_analysis) = &self.tab[main_def_id] else {
            let msg = "main is not a function".to_owned();
            return Err(AnalysisError(msg));
        };

        if main_analysis.return_type != ast::DataType::Void {
            let msg = format!(
                "the return type of main must be `void`, but it is `{}`",
                main_analysis.return_type,
            );
            return Err(AnalysisError(msg));
        }

        if main_analysis.param_count() != 0 {
            let msg = "the main function must not have parameters".to_owned();
            return Err(AnalysisError(msg));
        }

        Ok(FuncDefId(main_def_id))
    }
}

/// The top-level type that contains all program information that is collected during analysis.
///
/// Contains information about the main function and global variables that is required
/// for program startup, as well as the cumulative information collected for all definitions.
///
/// The structure can be accessed by the index operator. Providing a more specific
/// kind of id will return a more specific kind of result:
///
/// ```
/// use c1::ast::DefId;
/// use c1::analysis::*;
///
/// fn get_any(definitions: &ProgramInfo, id: DefId) -> &DefInfo {
///     &definitions[id]
/// }
///
/// fn get_func(definitions: &ProgramInfo, id: FuncDefId) -> &FuncInfo {
///     &definitions[id]
/// }
///
/// fn get_local_var(definitions: &ProgramInfo, id: LocalVarDefId) -> &VarInfo {
///     &definitions[id]
/// }
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct ProgramInfo {
    pub definitions: Definitions,
    pub global_vars: Vec<ast::GlobalVarItemId>,
    pub main_func: Option<FuncDefId>,
}

impl Index<ast::DefId> for ProgramInfo {
    type Output = DefInfo;

    fn index(&self, id: ast::DefId) -> &Self::Output {
        &self.definitions[id]
    }
}

impl Index<LocalVarDefId> for ProgramInfo {
    type Output = VarInfo;

    fn index(&self, id: LocalVarDefId) -> &Self::Output {
        &self.definitions[id]
    }
}

impl Index<FuncDefId> for ProgramInfo {
    type Output = FuncInfo;

    fn index(&self, id: FuncDefId) -> &Self::Output {
        &self.definitions[id]
    }
}

/// A human-readable compile-time error.
#[derive(Debug, Clone, PartialEq)]
pub struct AnalysisError(String);

impl fmt::Display for AnalysisError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<symtab::DefineError> for AnalysisError {
    fn from(err: symtab::DefineError) -> Self {
        let msg = format!("duplicate definition of {} in the same scope", err.0);
        AnalysisError(msg)
    }
}

impl From<symtab::ResolveError> for AnalysisError {
    fn from(err: symtab::ResolveError) -> Self {
        let msg = format!("cannot resolve {} in this scope", err.0);
        AnalysisError(msg)
    }
}
