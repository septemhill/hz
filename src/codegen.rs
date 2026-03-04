//! # LLVM IR Code Generator for Lang Programming Language
//!
//! This module generates LLVM IR from the AST.

use std::collections::HashMap;
use std::error::Error;

use inkwell::builder::Builder;
use inkwell::context::Context;
use inkwell::execution_engine::ExecutionEngine;
use inkwell::module::Module;
use inkwell::types::{BasicMetadataTypeEnum, BasicType, BasicTypeEnum};
use inkwell::values::{
    BasicMetadataValueEnum, BasicValue, BasicValueEnum, FunctionValue, PointerValue,
};

use crate::ast::*;
use crate::hir;
use crate::stdlib::StdLib;

/// Code generator context
pub struct CodeGenerator<'ctx> {
    pub context: &'ctx Context,
    pub module: Module<'ctx>,
    pub builder: Builder<'ctx>,
    pub execution_engine: ExecutionEngine<'ctx>,

    // Current function being built
    pub current_function: Option<FunctionValue<'ctx>>,

    // Basic block for control flow
    pub current_block: Option<inkwell::basic_block::BasicBlock<'ctx>>,

    // Variable scope (name -> LLVM value)
    variables: HashMap<String, PointerValue<'ctx>>,

    // Variable types (name -> Lang type) - for correct loading
    variable_types: HashMap<String, Type>,

    // Const variable scope (name -> LLVM value) - for compile-time error checking
    const_variables: HashMap<String, PointerValue<'ctx>>,

    // Return type of current function
    return_type: Option<Type>,

    // Standard library
    stdlib: StdLib,

    // Track imported packages (for duplicate checking)
    imported_packages: HashMap<String, String>, // alias -> package_name
}

/// Result of code generation
pub type CodegenResult<T> = Result<T, Box<dyn Error>>;

impl<'ctx> CodeGenerator<'ctx> {
    /// Create a new code generator
    pub fn new(context: &'ctx Context, module_name: &str, stdlib: StdLib) -> CodegenResult<Self> {
        let module = context.create_module(module_name);
        let execution_engine =
            module.create_jit_execution_engine(inkwell::OptimizationLevel::None)?;

        Ok(CodeGenerator {
            context,
            module,
            builder: context.create_builder(),
            execution_engine,
            current_function: None,
            current_block: None,
            variables: HashMap::new(),
            const_variables: HashMap::new(),
            variable_types: HashMap::new(),
            return_type: None,
            stdlib,
            imported_packages: HashMap::new(),
        })
    }

    /// Generate code for an HIR program
    pub fn generate_hir(&mut self, program: &hir::HirProgram) -> CodegenResult<()> {
        // Generate code for each function in HIR
        for hir_fn in &program.functions {
            self.generate_hir_function(hir_fn)?;
        }

        Ok(())
    }

    /// Generate code for an HIR function
    fn generate_hir_function(&mut self, hir_fn: &hir::HirFn) -> CodegenResult<()> {
        let function = self
            .module
            .get_function(&hir_fn.name)
            .ok_or(format!("Function not declared: {}", hir_fn.name))?;

        self.current_function = Some(function);
        self.return_type = Some(hir_fn.return_ty.clone());

        self.variables.clear();
        self.variable_types.clear();

        let entry_block = self.context.append_basic_block(function, "entry");
        self.builder.position_at_end(entry_block);
        self.current_block = Some(entry_block);

        for (i, (name, ty)) in hir_fn.params.iter().enumerate() {
            let param_value = function
                .get_nth_param(i as u32)
                .ok_or("Failed to get param")?;
            let llvm_type = self.llvm_type(ty);
            let alloca = self.builder.build_alloca(llvm_type, name)?;
            self.builder.build_store(alloca, param_value)?;
            self.variables.insert(name.clone(), alloca);
            self.variable_types.insert(name.clone(), ty.clone());
        }

        // For void functions, we need to handle implicit returns properly
        // For main function, if return type is Void, treat it as i64 (return 0)
        let is_void = hir_fn.return_ty == Type::Void && hir_fn.name != "main";

        // Track statements
        let stmt_count = hir_fn.body.len();
        for (i, stmt) in hir_fn.body.iter().enumerate() {
            // Check if this is the last statement in a void function
            let is_last = i == stmt_count - 1;

            match stmt {
                hir::HirStmt::Return(_, _) => {
                    // Explicit return - generate it and we're done
                    self.generate_hir_stmt(stmt)?;
                    return Ok(());
                }
                _ if is_last && is_void => {
                    // Last statement in void function
                    // For expression statements, call generate_hir_expr directly to avoid
                    // generating a return statement from the Call expression
                    match stmt {
                        hir::HirStmt::Expr(expr) => {
                            // Just evaluate for side effects, don't add a return
                            let _ = self.generate_hir_expr(expr);
                        }
                        _ => {
                            self.generate_hir_stmt(stmt)?;
                        }
                    }
                    // Return void
                    self.builder.build_return(None)?;
                    return Ok(());
                }
                _ if is_last && !is_void => {
                    // Last statement in non-void function - use value as return
                    self.generate_hir_stmt(stmt)?;
                    // For main, always return 0
                    self.builder
                        .build_return(Some(&self.context.i64_type().const_int(0, false)))?;
                    return Ok(());
                }
                _ => {
                    self.generate_hir_stmt(stmt)?;
                }
            }
        }

        // If we get here, we didn't return in the loop
        if is_void {
            self.builder.build_return(None)?;
        } else if hir_fn.name == "main" {
            // main function without explicit return - return 0
            self.builder
                .build_return(Some(&self.context.i64_type().const_int(0, false)))?;
        } else {
            // Non-void function without return - return 0
            self.builder
                .build_return(Some(&self.context.i64_type().const_int(0, false)))?;
        }

        self.current_function = None;
        self.current_block = None;
        Ok(())
    }

    fn generate_hir_stmt(&mut self, stmt: &hir::HirStmt) -> CodegenResult<()> {
        match stmt {
            hir::HirStmt::Expr(expr) => {
                self.generate_hir_expr(expr)?;
                Ok(())
            }
            hir::HirStmt::Let {
                name,
                ty,
                value,
                mutability,
                ..
            } => {
                let llvm_type = self.llvm_type(ty);
                let alloca = self.builder.build_alloca(llvm_type, name)?;
                if let Some(val) = value {
                    let llvm_val = self.generate_hir_expr(val)?;
                    self.builder.build_store(alloca, llvm_val)?;
                }
                self.variables.insert(name.clone(), alloca);
                self.variable_types.insert(name.clone(), ty.clone());
                if *mutability == Mutability::Const {
                    self.const_variables.insert(name.clone(), alloca);
                }
                Ok(())
            }
            hir::HirStmt::Assign { target, value, .. } => {
                let ptr = self.variables.get(target).ok_or("Var not found")?.clone();
                let llvm_val = self.generate_hir_expr(value)?;
                self.builder.build_store(ptr, llvm_val)?;
                Ok(())
            }
            hir::HirStmt::Return(value, _) => {
                if let Some(val) = value {
                    let llvm_val = self.generate_hir_expr(val)?;
                    self.builder.build_return(Some(&llvm_val))?;
                } else {
                    self.builder.build_return(None)?;
                }
                Ok(())
            }
            hir::HirStmt::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                let cond_val = self.generate_hir_expr(condition)?;
                let function = self.current_function.unwrap();
                let then_block = self.context.append_basic_block(function, "then");
                let else_block = self.context.append_basic_block(function, "else");
                let merge_block = self.context.append_basic_block(function, "cont");

                let zero = self.context.i64_type().const_int(0, false);
                let is_true = self.builder.build_int_compare(
                    inkwell::IntPredicate::NE,
                    cond_val.into_int_value(),
                    zero,
                    "is_true",
                )?;
                self.builder
                    .build_conditional_branch(is_true, then_block, else_block)?;

                self.builder.position_at_end(then_block);
                self.generate_hir_stmt(then_branch)?;
                self.builder.build_unconditional_branch(merge_block)?;

                self.builder.position_at_end(else_block);
                if let Some(eb) = else_branch {
                    self.generate_hir_stmt(eb)?;
                }
                self.builder.build_unconditional_branch(merge_block)?;

                self.builder.position_at_end(merge_block);
                Ok(())
            }
            hir::HirStmt::While {
                condition, body, ..
            } => {
                let function = self.current_function.unwrap();
                let cond_block = self.context.append_basic_block(function, "while_cond");
                let body_block = self.context.append_basic_block(function, "while_body");
                let end_block = self.context.append_basic_block(function, "while_end");

                // Jump to condition block
                self.builder.build_unconditional_branch(cond_block)?;

                // Condition block
                self.builder.position_at_end(cond_block);
                let cond_val = self.generate_hir_expr(condition)?;
                let zero = self.context.i64_type().const_int(0, false);
                let is_true = self.builder.build_int_compare(
                    inkwell::IntPredicate::NE,
                    cond_val.into_int_value(),
                    zero,
                    "while_is_true",
                )?;
                self.builder
                    .build_conditional_branch(is_true, body_block, end_block)?;

                // Body block
                self.builder.position_at_end(body_block);
                self.generate_hir_stmt(body)?;
                self.builder.build_unconditional_branch(cond_block)?;

                // End block
                self.builder.position_at_end(end_block);
                Ok(())
            }
            hir::HirStmt::Switch {
                condition, cases, ..
            } => {
                // For switch statements, generate a series of if-else branches
                let function = self.current_function.unwrap();
                let end_block = self.context.append_basic_block(function, "switch_end");

                let cond_val = self.generate_hir_expr(condition)?;

                // Generate conditions for each case
                for case in cases {
                    let case_block = self.context.append_basic_block(function, "case");

                    // For each pattern in the case
                    for pattern in &case.patterns {
                        // Compare pattern with condition
                        let pattern_val = self.generate_hir_expr(pattern)?;
                        let is_eq = self.builder.build_int_compare(
                            inkwell::IntPredicate::EQ,
                            cond_val.into_int_value(),
                            pattern_val.into_int_value(),
                            "case_cmp",
                        )?;

                        // Create a block for this pattern
                        let pattern_block = self.context.append_basic_block(function, "pattern");
                        self.builder.build_conditional_branch(
                            is_eq,
                            pattern_block,
                            pattern_block,
                        )?;

                        // Pattern block
                        self.builder.position_at_end(pattern_block);
                    }

                    // Case block
                    self.builder.position_at_end(case_block);
                    self.generate_hir_stmt(&case.body)?;
                    self.builder.build_unconditional_branch(end_block)?;
                }

                // End block
                self.builder.position_at_end(end_block);
                Ok(())
            }
        }
    }

    fn generate_hir_expr(&mut self, expr: &hir::HirExpr) -> CodegenResult<BasicValueEnum<'ctx>> {
        match expr {
            hir::HirExpr::Int(v, _, _) => {
                Ok(self.context.i64_type().const_int(*v as u64, false).into())
            }
            hir::HirExpr::Bool(v, _, _) => Ok(self
                .context
                .bool_type()
                .const_int(if *v { 1 } else { 0 }, false)
                .into()),
            hir::HirExpr::String(v, _, _) => {
                // For string literals, create a global string and return its pointer
                let str_val = unsafe { self.builder.build_global_string(v, "str") }?;
                Ok(str_val.as_basic_value_enum())
            }
            hir::HirExpr::Char(v, _, _) => {
                // Characters are stored as i64 in our implementation
                Ok(self.context.i64_type().const_int(*v as u64, false).into())
            }
            hir::HirExpr::Null(_, _) => {
                // Return null pointer (i64* null)
                let ptr_type = self
                    .context
                    .ptr_type(inkwell::AddressSpace::default());
                Ok(ptr_type.const_null().into())
            }
            hir::HirExpr::Tuple { vals, ty, .. } => {
                // Create an LLVM struct from tuple values
                let struct_type = self.llvm_type(ty);
                let mut elements: Vec<BasicValueEnum> = Vec::new();
                for v in vals {
                    elements.push(self.generate_hir_expr(v)?);
                }
                // Build struct value
                let struct_val = self.context.const_struct(&elements, false);
                // We need to store it to memory to return as pointer
                let alloca = self.builder.build_alloca(struct_type, "tuple")?;
                self.builder.build_store(alloca, struct_val)?;
                Ok(self
                    .builder
                    .build_load(struct_type, alloca, "tuple_load")?
                    .into())
            }
            hir::HirExpr::TupleIndex {
                tuple, index, ty, ..
            } => {
                // Get the tuple value
                let tuple_val = self.generate_hir_expr(tuple)?;
                // Extract the element at index
                let llvm_type = self.llvm_type(ty);
                let alloca = self
                    .builder
                    .build_alloca(tuple_val.get_type(), "tuple_idx_temp")?;
                self.builder.build_store(alloca, tuple_val)?;
                let extracted = self.builder.build_extract_value(
                    self.builder
                        .build_load(tuple_val.get_type(), alloca, "t")?
                        .into_struct_value(),
                    *index as u32,
                    "tuple_elem",
                )?;
                Ok(extracted.into())
            }
            hir::HirExpr::Array { vals, ty, .. } => {
                // For array literals, return 0 for now
                // A full implementation would create a vector or heap-allocated array
                Ok(self.context.i64_type().const_int(0, false).into())
            }
            hir::HirExpr::Ident(name, _, _) => {
                let ptr = self.variables.get(name).ok_or("Var not found")?;
                let ty = self.variable_types.get(name).unwrap();
                let llvm_type = self.llvm_type(ty);
                Ok(self.builder.build_load(llvm_type, *ptr, name)?.into())
            }
            hir::HirExpr::Binary {
                op,
                left,
                right,
                ty,
                ..
            } => {
                let l = self.generate_hir_expr(left)?;
                let r = self.generate_hir_expr(right)?;

                // Handle different types
                let val = match op {
                    BinaryOp::Add => {
                        let l_int = l.into_int_value();
                        let r_int = r.into_int_value();
                        self.builder.build_int_add(l_int, r_int, "add")?.into()
                    }
                    BinaryOp::Sub => {
                        let l_int = l.into_int_value();
                        let r_int = r.into_int_value();
                        self.builder.build_int_sub(l_int, r_int, "sub")?.into()
                    }
                    BinaryOp::Mul => {
                        let l_int = l.into_int_value();
                        let r_int = r.into_int_value();
                        self.builder.build_int_mul(l_int, r_int, "mul")?.into()
                    }
                    BinaryOp::Div => {
                        let l_int = l.into_int_value();
                        let r_int = r.into_int_value();
                        self.builder
                            .build_int_unsigned_div(l_int, r_int, "div")?
                            .into()
                    }
                    BinaryOp::Mod => {
                        let l_int = l.into_int_value();
                        let r_int = r.into_int_value();
                        self.builder
                            .build_int_unsigned_rem(l_int, r_int, "mod")?
                            .into()
                    }
                    BinaryOp::Eq => {
                        let l_int = l.into_int_value();
                        let r_int = r.into_int_value();
                        let cmp = self.builder.build_int_compare(
                            inkwell::IntPredicate::EQ,
                            l_int,
                            r_int,
                            "eq",
                        )?;
                        cmp.into()
                    }
                    BinaryOp::Ne => {
                        let l_int = l.into_int_value();
                        let r_int = r.into_int_value();
                        let cmp = self.builder.build_int_compare(
                            inkwell::IntPredicate::NE,
                            l_int,
                            r_int,
                            "ne",
                        )?;
                        cmp.into()
                    }
                    BinaryOp::Lt => {
                        let l_int = l.into_int_value();
                        let r_int = r.into_int_value();
                        let cmp = self.builder.build_int_compare(
                            inkwell::IntPredicate::ULT,
                            l_int,
                            r_int,
                            "lt",
                        )?;
                        cmp.into()
                    }
                    BinaryOp::Gt => {
                        let l_int = l.into_int_value();
                        let r_int = r.into_int_value();
                        let cmp = self.builder.build_int_compare(
                            inkwell::IntPredicate::UGT,
                            l_int,
                            r_int,
                            "gt",
                        )?;
                        cmp.into()
                    }
                    BinaryOp::Le => {
                        let l_int = l.into_int_value();
                        let r_int = r.into_int_value();
                        let cmp = self.builder.build_int_compare(
                            inkwell::IntPredicate::ULE,
                            l_int,
                            r_int,
                            "le",
                        )?;
                        cmp.into()
                    }
                    BinaryOp::Ge => {
                        let l_int = l.into_int_value();
                        let r_int = r.into_int_value();
                        let cmp = self.builder.build_int_compare(
                            inkwell::IntPredicate::UGE,
                            l_int,
                            r_int,
                            "ge",
                        )?;
                        cmp.into()
                    }
                    BinaryOp::And => {
                        let l_int = l.into_int_value();
                        let r_int = r.into_int_value();
                        let and_val = self.builder.build_and(l_int, r_int, "and")?;
                        and_val.into()
                    }
                    BinaryOp::Or => {
                        let l_int = l.into_int_value();
                        let r_int = r.into_int_value();
                        let or_val = self.builder.build_or(l_int, r_int, "or")?;
                        or_val.into()
                    }
                    BinaryOp::Range => {
                        // For range, we'll just return 0 for now
                        // A full implementation would create a range object
                        self.context.i64_type().const_int(0, false).into()
                    }
                };
                Ok(val)
            }
            hir::HirExpr::Unary { op, expr, ty, .. } => {
                let e = self.generate_hir_expr(expr)?;
                let val = match op {
                    UnaryOp::Neg => {
                        let e_int = e.into_int_value();
                        self.builder.build_int_neg(e_int, "neg")?.into()
                    }
                    UnaryOp::Pos => {
                        // Positive is a no-op
                        e
                    }
                    UnaryOp::Not => {
                        let e_int = e.into_int_value();
                        let zero = self.context.i64_type().const_int(0, false);
                        let cmp = self.builder.build_int_compare(
                            inkwell::IntPredicate::EQ,
                            e_int,
                            zero,
                            "not",
                        )?;
                        cmp.into()
                    }
                };
                Ok(val)
            }
            hir::HirExpr::Call {
                name,
                namespace,
                args,
                ..
            } => {
                if namespace.as_deref() == Some("io") && name == "println" {
                    return self.generate_hir_io_println(args);
                }
                let function = self
                    .module
                    .get_function(name)
                    .ok_or(format!("Fn not found: {}", name))?;
                let llvm_args: Vec<BasicMetadataValueEnum> = args
                    .iter()
                    .map(|a| self.generate_hir_expr(a).unwrap().into())
                    .collect();
                let call_result = self.builder.build_call(function, &llvm_args, "call")?;
                let result = match call_result.try_as_basic_value() {
                    _ => self.context.i64_type().const_int(0, false).into(),
                };
                Ok(result)
            }
            hir::HirExpr::If {
                condition,
                then_branch,
                else_branch,
                ty,
                ..
            } => {
                // For if as expression, we need to handle phi nodes
                // For simplicity, we'll evaluate both branches and select based on condition
                let cond_val = self.generate_hir_expr(condition)?;
                let function = self.current_function.unwrap();
                let then_block = self.context.append_basic_block(function, "then");
                let else_block = self.context.append_basic_block(function, "else");
                let merge_block = self.context.append_basic_block(function, "cont");

                let zero = self.context.i64_type().const_int(0, false);
                let is_true = self.builder.build_int_compare(
                    inkwell::IntPredicate::NE,
                    cond_val.into_int_value(),
                    zero,
                    "is_true",
                )?;
                self.builder
                    .build_conditional_branch(is_true, then_block, else_block)?;

                // Then branch
                self.builder.position_at_end(then_block);
                let then_val = self.generate_hir_expr(then_branch)?;
                self.builder.build_unconditional_branch(merge_block)?;

                // Else branch
                self.builder.position_at_end(else_block);
                let else_val = self.generate_hir_expr(else_branch)?;
                self.builder.build_unconditional_branch(merge_block)?;

                // Merge - create phi node for the result
                self.builder.position_at_end(merge_block);
                let result_type = self.llvm_type(ty);
                let phi = self.builder.build_phi(result_type, "if_result")?;
                phi.add_incoming(&[(&then_val, then_block), (&else_val, else_block)]);

                Ok(phi.as_basic_value())
            }
            hir::HirExpr::Block { stmts, expr, .. } => {
                // Evaluate all statements in the block
                for stmt in stmts {
                    self.generate_hir_stmt(stmt)?;
                }
                // If there's an expression, return its value
                if let Some(e) = expr {
                    self.generate_hir_expr(e)
                } else {
                    Ok(self.context.i64_type().const_int(0, false).into())
                }
            }
            hir::HirExpr::MemberAccess {
                object, member, ty, ..
            } => {
                // For member access, we need to get the struct and extract the field
                let obj_val = self.generate_hir_expr(object)?;
                let struct_type = obj_val.get_type();

                // For now, we'll assume the member is a field index (0, 1, 2, ...)
                // This is a simplification - a full implementation would look up the field name
                let field_idx: u32 = member.parse().unwrap_or(0);

                let alloca = self.builder.build_alloca(struct_type, "member_temp")?;
                self.builder.build_store(alloca, obj_val)?;
                let loaded = self
                    .builder
                    .build_load(struct_type, alloca, "member_load")?;
                let extracted = self.builder.build_extract_value(
                    loaded.into_struct_value(),
                    field_idx,
                    member,
                )?;

                Ok(extracted.into())
            }
            hir::HirExpr::Struct {
                name, fields, ty, ..
            } => {
                // Create a struct instance
                let struct_type = self.llvm_type(ty);

                // Get field types
                let mut field_values: Vec<BasicValueEnum> = Vec::new();
                for (_, v) in fields {
                    field_values.push(self.generate_hir_expr(v)?);
                }

                let struct_val = self.context.const_struct(&field_values, false);
                let alloca = self.builder.build_alloca(struct_type, name)?;
                self.builder.build_store(alloca, struct_val)?;

                Ok(self
                    .builder
                    .build_load(struct_type, alloca, "struct_load")?
                    .into())
            }
        }
    }

    fn generate_hir_io_println(
        &mut self,
        args: &[hir::HirExpr],
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        let printf_type = self.context.i64_type().fn_type(
            &[self
                .context
                .ptr_type(inkwell::AddressSpace::default())
                .into()],
            true,
        );
        let printf = self.module.get_function("printf").unwrap_or_else(|| {
            self.module.add_function(
                "printf",
                printf_type,
                Some(inkwell::module::Linkage::External),
            )
        });

        if args.is_empty() {
            let empty_str = unsafe { self.builder.build_global_string("\n", "empty") }?;
            self.builder
                .build_call(printf, &[empty_str.as_basic_value_enum().into()], "")?;
        } else {
            let arg = self.generate_hir_expr(&args[0])?;
            self.builder.build_call(printf, &[arg.into()], "")?;
        }
        Ok(self.context.i64_type().const_int(0, false).into())
    }

    /// Declare a struct type in LLVM
    pub fn declare_struct(&mut self, struct_def: &StructDef) -> CodegenResult<()> {
        // Only generate code for exported (public) structs
        if !struct_def.visibility.is_public() {
            return Ok(());
        }

        let struct_name = &struct_def.name;

        // Create struct type
        let field_types: Vec<BasicTypeEnum> = struct_def
            .fields
            .iter()
            .map(|f| self.llvm_type(&f.ty))
            .collect();

        let struct_type = self.context.opaque_struct_type(struct_name);
        struct_type.set_body(&field_types, false);

        Ok(())
    }

    /// Declare an enum type in LLVM
    pub fn declare_enum(&mut self, enum_def: &EnumDef) -> CodegenResult<()> {
        // Only generate code for exported (public) enums
        if !enum_def.visibility.is_public() {
            return Ok(());
        }

        let _enum_name = &enum_def.name;

        // For enums, we use an integer type as the representation
        // In a full implementation, we'd use a tagged union
        let _enum_type = self.context.i64_type();

        Ok(())
    }

    /// Declare a function (create function signature)
    pub fn declare_function(&mut self, fn_def: &FnDef) -> CodegenResult<()> {
        // For main function without return type, default to i64
        // For other functions without return type, default to void
        let default_return_type = if fn_def.name == "main" && fn_def.return_ty.is_none() {
            Type::I64
        } else {
            Type::Void
        };
        let return_type = self.llvm_type(fn_def.return_ty.as_ref().unwrap_or(&default_return_type));
        let param_types: Vec<BasicMetadataTypeEnum> = fn_def
            .params
            .iter()
            .map(|p| self.llvm_type(&p.ty).into())
            .collect();

        let fn_type = return_type.fn_type(&param_types, false);
        self.module.add_function(&fn_def.name, fn_type, None);

        Ok(())
    }

    /// Generate code for a function
    fn generate_function(&mut self, fn_def: &FnDef) -> CodegenResult<()> {
        // Get or create the function
        let function = self
            .module
            .get_function(&fn_def.name)
            .ok_or(format!("Function not declared: {}", fn_def.name))?;

        self.current_function = Some(function);
        self.return_type = fn_def.return_ty.clone();

        // Clear variable scope for this function
        self.variables.clear();
        self.variable_types.clear();

        // Create entry basic block
        let entry_block = self.context.append_basic_block(function, "entry");
        self.builder.position_at_end(entry_block);
        self.current_block = Some(entry_block);

        // Allocate parameters
        for (i, param) in fn_def.params.iter().enumerate() {
            let param_value = function.get_nth_param(i as u32).ok_or(format!(
                "Failed to get parameter {} for function {}",
                i, fn_def.name
            ))?;

            // Create alloca for the parameter
            let param_type = self.llvm_type(&param.ty);
            let alloca = self.builder.build_alloca(param_type, &param.name)?;
            self.builder.build_store(alloca, param_value)?;
            self.variables.insert(param.name.clone(), alloca);
        }

        // Generate statements in the function body
        for stmt in &fn_def.body {
            self.generate_stmt(stmt)?;
        }

        // If the function doesn't have a return statement, add implicit return
        // For main function without return type, default to returning 0 (i64)
        // For other functions without return type, default to void
        if fn_def.return_ty == Some(Type::Void) {
            self.builder.build_return(None)?;
        } else if fn_def.return_ty.is_none() {
            if fn_def.name == "main" {
                self.builder
                    .build_return(Some(&self.context.i64_type().const_int(0, false)))?;
            } else {
                self.builder.build_return(None)?;
            }
        }

        self.current_function = None;
        self.current_block = None;

        Ok(())
    }

    /// Generate code for a statement
    fn generate_stmt(&mut self, stmt: &Stmt) -> CodegenResult<()> {
        match stmt {
            Stmt::Expr { expr, .. } => {
                self.generate_expr(expr)?;
                Ok(())
            }
            Stmt::Import { packages, span: _ } => {
                // Import statements: handle duplicates and aliases
                for (alias, package_name) in packages {
                    let namespace = alias.as_deref().unwrap_or(package_name.as_str());

                    eprintln!(
                        "DEBUG: Processing import: namespace={}, package={}",
                        namespace, package_name
                    );
                    eprintln!(
                        "DEBUG: imported_packages before: {:?}",
                        self.imported_packages.keys().collect::<Vec<_>>()
                    );

                    // Check for duplicate import
                    if self.imported_packages.contains_key(namespace) {
                        return Err(format!(
                            "Duplicate import: '{}' is already imported",
                            namespace
                        )
                        .into());
                    }

                    // Also check if the same package is imported under a different name
                    for (existing_alias, existing_package) in &self.imported_packages {
                        if existing_package.as_str() == package_name.as_str()
                            && Some(existing_alias.as_str()) != alias.as_deref()
                        {
                            return Err(format!(
                                "Package '{}' is already imported as '{}'",
                                package_name, existing_alias
                            )
                            .into());
                        }
                    }

                    // Track this import
                    self.imported_packages
                        .insert(namespace.to_string(), package_name.clone());

                    // Try to load the package
                    if let Err(e) = self.stdlib.load_package(package_name) {
                        return Err(format!("Import error: {}", e).into());
                    }
                }
                Ok(())
            }
            Stmt::Let {
                mutability,
                name,
                names,
                ty,
                value,
                visibility: _,
                span: _,
            } => {
                // If value exists, generate it first to get the actual type
                let llvm_val = if let Some(val) = value {
                    Some(self.generate_expr(val)?)
                } else {
                    None
                };

                // Handle tuple destructuring: const (a, b, c) = tuple_expr
                if let Some(names) = &names {
                    // Tuple destructuring
                    let tuple_val = llvm_val.ok_or("Tuple destructuring requires a value")?;

                    // Get the tuple as a struct value
                    let struct_val = match tuple_val {
                        BasicValueEnum::StructValue(sv) => sv,
                        _ => return Err("Tuple destructuring requires a tuple value".into()),
                    };

                    let num_names = names.len();

                    // Get element types by extracting first element and getting its type
                    // We need to know how many elements there are
                    let mut element_types: Vec<BasicTypeEnum<'ctx>> = Vec::new();

                    // Try to get element types by iterating - we assume tuple has at most num_names elements
                    // We'll try to extract up to num_names elements to validate
                    for i in 0..num_names {
                        // We can't directly get element type from struct type in inkwell
                        // Instead, we extract each element and use its type
                        if let Ok(elem) = self.builder.build_extract_value(
                            struct_val,
                            i as u32,
                            &format!("tuple_elem{}", i),
                        ) {
                            element_types.push(elem.get_type());
                        } else {
                            break;
                        }
                    }

                    let num_elements = element_types.len();

                    if num_names != num_elements {
                        return Err(format!(
                            "Tuple destructuring: expected {} elements, got {}",
                            num_names, num_elements
                        )
                        .into());
                    }

                    // Process each name in the destructuring
                    for (i, name_opt) in names.iter().enumerate() {
                        if let Some(var_name) = name_opt {
                            let elem_type = element_types[i];
                            let elem = self.builder.build_extract_value(
                                struct_val,
                                i as u32,
                                &format!("tuple_elem{}", i),
                            )?;

                            // Create variable
                            let alloca = self.builder.build_alloca(elem_type, var_name)?;
                            self.builder.build_store(alloca, elem)?;

                            self.variables.insert(var_name.clone(), alloca);
                            self.variable_types
                                .insert(var_name.clone(), self.llvm_type_to_lang(&elem_type));

                            if *mutability == Mutability::Const {
                                self.const_variables.insert(var_name.clone(), alloca);
                            }
                        }
                        // If name_opt is None, we're ignoring this element - no code to generate
                    }

                    return Ok(());
                }

                // Determine the type: use explicit type or infer from generated value
                // First determine the Lang type
                let lang_type = match ty {
                    Some(explicit_ty) => explicit_ty.clone(),
                    None => {
                        if let Some(ref val) = llvm_val {
                            // Infer type from LLVM value using helper function
                            let llvm_type = val.get_type();
                            self.llvm_type_to_lang(&llvm_type)
                        } else {
                            Type::I64
                        }
                    }
                };

                let var_type = self.llvm_type(&lang_type);

                let alloca = self.builder.build_alloca(var_type, name)?;

                if let Some(val) = llvm_val {
                    self.builder.build_store(alloca, val)?;
                }

                self.variables.insert(name.clone(), alloca);

                // Track the Lang type for correct loading later
                self.variable_types.insert(name.clone(), lang_type);

                // Track const variables for compile-time error checking
                if *mutability == Mutability::Const {
                    self.const_variables.insert(name.clone(), alloca);
                }

                Ok(())
            }
            Stmt::Assign {
                target,
                op,
                value,
                span: _,
            } => {
                // Check if trying to reassign a const variable (compile-time error)
                if self.const_variables.contains_key(target) {
                    return Err(format!("Cannot reassign constant variable '{}'", target).into());
                }

                // Get the pointer first to avoid borrow issues
                let target_ptr = self
                    .variables
                    .get(target)
                    .ok_or(format!("Variable not found: {}", target))?
                    .clone();

                let llvm_value = self.generate_expr(value)?;

                match op {
                    AssignOp::Assign => {
                        self.builder.build_store(target_ptr, llvm_value)?;
                    }
                    AssignOp::AddAssign => {
                        let current = self.builder.build_load(
                            self.llvm_type(&Type::I64),
                            target_ptr,
                            "tmp",
                        )?;
                        let result = self.builder.build_int_add(
                            current.into_int_value(),
                            llvm_value.into_int_value(),
                            "addtmp",
                        )?;
                        self.builder.build_store(target_ptr, result)?;
                    }
                    AssignOp::SubAssign => {
                        let current = self.builder.build_load(
                            self.llvm_type(&Type::I64),
                            target_ptr,
                            "tmp",
                        )?;
                        let result = self.builder.build_int_sub(
                            current.into_int_value(),
                            llvm_value.into_int_value(),
                            "subtmp",
                        )?;
                        self.builder.build_store(target_ptr, result)?;
                    }
                    AssignOp::MulAssign => {
                        let current = self.builder.build_load(
                            self.llvm_type(&Type::I64),
                            target_ptr,
                            "tmp",
                        )?;
                        let result = self.builder.build_int_mul(
                            current.into_int_value(),
                            llvm_value.into_int_value(),
                            "multmp",
                        )?;
                        self.builder.build_store(target_ptr, result)?;
                    }
                    AssignOp::DivAssign => {
                        let current = self.builder.build_load(
                            self.llvm_type(&Type::I64),
                            target_ptr,
                            "tmp",
                        )?;
                        let result = self.builder.build_int_signed_div(
                            current.into_int_value(),
                            llvm_value.into_int_value(),
                            "divtmp",
                        )?;
                        self.builder.build_store(target_ptr, result)?;
                    }
                }

                Ok(())
            }
            Stmt::Return { value, .. } => {
                if let Some(val) = value {
                    let llvm_val = self.generate_expr(val)?;
                    self.builder.build_return(Some(&llvm_val))?;
                } else {
                    self.builder.build_return(None)?;
                }
                Ok(())
            }
            Stmt::Block { stmts, .. } => {
                for s in stmts {
                    self.generate_stmt(s)?;
                }
                Ok(())
            }
            Stmt::If {
                condition,
                capture,
                then_branch,
                else_branch,
                ..
            } => self.generate_if(condition, capture, then_branch, else_branch.as_deref()),
            Stmt::While {
                condition,
                capture,
                body,
                ..
            } => self.generate_while(condition, capture, body),
            Stmt::Loop { body, .. } => self.generate_loop(body),
            Stmt::For { .. } => todo!("Codegen for For loops not implemented"),
            Stmt::Switch { .. } => todo!("Codegen for Switch statements not implemented"),
        }
    }

    /// Generate code for an expression and return its LLVM value
    fn generate_expr(&mut self, expr: &Expr) -> CodegenResult<BasicValueEnum<'ctx>> {
        match expr {
            Expr::Int(value, _) => {
                let i64_type = self.context.i64_type();
                Ok(i64_type.const_int(*value as u64, false).into())
            }
            Expr::Bool(value, _) => {
                let i1_type = self.context.bool_type();
                Ok(i1_type.const_int(if *value { 1 } else { 0 }, false).into())
            }
            Expr::String(value, _) => {
                // Create a global string constant (unsafe)
                let string_const =
                    unsafe { self.builder.build_global_string(value.as_str(), "str") }?;
                Ok(string_const.as_basic_value_enum())
            }
            Expr::Null(_) => {
                // Null is represented as a struct { value, is_valid } with is_valid = false
                // We'll use i64 as placeholder value and false for is_valid
                let i64_type = self.context.i64_type();
                let bool_type = self.context.bool_type();
                let null_struct = self
                    .context
                    .struct_type(&[i64_type.into(), bool_type.into()], false);
                Ok(null_struct.const_zero().into())
            }
            Expr::Tuple(exprs, _) => {
                // Generate a tuple: create a struct with all elements
                let mut values: Vec<BasicValueEnum<'ctx>> = Vec::new();
                for expr in exprs {
                    values.push(self.generate_expr(expr)?);
                }

                // Create struct type from the values
                let mut element_types: Vec<BasicTypeEnum<'ctx>> = Vec::new();
                for val in &values {
                    element_types.push(val.get_type());
                }
                let struct_type = self.context.struct_type(&element_types, false);

                // Build the struct value using aggregate values
                let mut struct_val: inkwell::values::AggregateValueEnum =
                    struct_type.const_zero().into();
                for (i, val) in values.iter().enumerate() {
                    struct_val = self
                        .builder
                        .build_insert_value(
                            struct_val.into_struct_value(),
                            *val,
                            i as u32,
                            "tuple_elem",
                        )?
                        .into();
                }
                Ok(struct_val.as_basic_value_enum())
            }
            Expr::TupleIndex { tuple, index, .. } => {
                // Generate tuple index access: tuple.0, tuple.1, etc.
                let tuple_val = self.generate_expr(tuple)?;

                // Try to extract directly
                match tuple_val {
                    // If it's already a struct value, extract directly
                    BasicValueEnum::StructValue(sv) => {
                        let elem = self.builder.build_extract_value(
                            sv,
                            *index as u32,
                            &format!("tuple_idx{}", index),
                        )?;
                        Ok(elem.into())
                    }
                    // If it's a pointer, use the stored variable type to load correctly
                    BasicValueEnum::PointerValue(ptr) => {
                        // Get the variable name if this is an Ident expression
                        let var_name = match tuple.as_ref() {
                            Expr::Ident(n, _) => Some(n.clone()),
                            _ => None,
                        };

                        if let Some(name) = var_name {
                            if let Some(var_type) = self.variable_types.get(&name) {
                                // Load with the correct type
                                let load_type = self.llvm_type(var_type);
                                let loaded =
                                    self.builder.build_load(load_type, ptr, "tuple_load")?;
                                // Extract the element
                                let elem = self.builder.build_extract_value(
                                    loaded.into_struct_value(),
                                    *index as u32,
                                    &format!("tuple_idx{}", index),
                                )?;
                                return Ok(elem.into());
                            }
                        }
                        // Fallback: try loading as i64
                        let loaded =
                            self.builder
                                .build_load(self.context.i64_type(), ptr, "tuple_load")?;
                        Ok(loaded.into())
                    }
                    // For other cases (like int value), try to extract (will fail gracefully)
                    _ => Err("Tuple index access requires a tuple or pointer to tuple".into()),
                }
            }
            Expr::Ident(name, _) => {
                let ptr = self
                    .variables
                    .get(name)
                    .ok_or(format!("Variable not found: {}", name))?;
                // Use the stored variable type for loading, default to i64
                let load_type = if let Some(var_type) = self.variable_types.get(name) {
                    self.llvm_type(var_type)
                } else {
                    self.llvm_type(&Type::I64)
                };
                let load = self.builder.build_load(load_type, *ptr, name)?;
                Ok(load.into())
            }
            Expr::Binary {
                op, left, right, ..
            } => self.generate_binary_op(*op, left, right),
            Expr::Unary { op, expr, .. } => self.generate_unary_op(*op, expr),
            Expr::Call {
                name,
                namespace,
                args,
                ..
            } => self.generate_call(name, namespace.as_deref(), args),
            Expr::Array(_, _) => todo!("Codegen for Array literals not implemented"),
            Expr::Char(_, _) => todo!("Codegen for character literals not implemented"),
            Expr::If {
                condition,
                capture,
                then_branch,
                else_branch,
                ..
            } => self.generate_expr_if(condition, capture, then_branch, else_branch),
            Expr::Block { stmts, .. } => {
                let mut last_val = None;
                for stmt in stmts {
                    match stmt {
                        Stmt::Expr { expr, .. } => {
                            last_val = Some(self.generate_expr(expr)?);
                        }
                        _ => {
                            self.generate_stmt(stmt)?;
                            last_val = None;
                        }
                    }
                }
                Ok(last_val.unwrap_or_else(|| self.context.i64_type().const_int(0, false).into()))
            }
            Expr::MemberAccess { .. } => todo!("Codegen for MemberAccess not implemented"),
            Expr::Struct { .. } => todo!("Codegen for Struct not implemented"),
        }
    }

    /// Generate binary operation
    fn generate_binary_op(
        &mut self,
        op: BinaryOp,
        left: &Expr,
        right: &Expr,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        let left_val = self.generate_expr(left)?;
        let right_val = self.generate_expr(right)?;

        // For now, assume i64 operations
        let i64_type = self.context.i64_type();

        let result: BasicValueEnum = match op {
            BinaryOp::Add => self
                .builder
                .build_int_add(left_val.into_int_value(), right_val.into_int_value(), "add")?
                .into(),
            BinaryOp::Sub => self
                .builder
                .build_int_sub(left_val.into_int_value(), right_val.into_int_value(), "sub")?
                .into(),
            BinaryOp::Mul => self
                .builder
                .build_int_mul(left_val.into_int_value(), right_val.into_int_value(), "mul")?
                .into(),
            BinaryOp::Div => self
                .builder
                .build_int_signed_div(left_val.into_int_value(), right_val.into_int_value(), "div")?
                .into(),
            BinaryOp::Mod => self
                .builder
                .build_int_signed_rem(left_val.into_int_value(), right_val.into_int_value(), "rem")?
                .into(),
            BinaryOp::Eq => self
                .builder
                .build_int_compare(
                    inkwell::IntPredicate::EQ,
                    left_val.into_int_value(),
                    right_val.into_int_value(),
                    "eq",
                )?
                .into(),
            BinaryOp::Ne => self
                .builder
                .build_int_compare(
                    inkwell::IntPredicate::NE,
                    left_val.into_int_value(),
                    right_val.into_int_value(),
                    "ne",
                )?
                .into(),
            BinaryOp::Lt => self
                .builder
                .build_int_compare(
                    inkwell::IntPredicate::SLT,
                    left_val.into_int_value(),
                    right_val.into_int_value(),
                    "lt",
                )?
                .into(),
            BinaryOp::Gt => self
                .builder
                .build_int_compare(
                    inkwell::IntPredicate::SGT,
                    left_val.into_int_value(),
                    right_val.into_int_value(),
                    "gt",
                )?
                .into(),
            BinaryOp::Le => self
                .builder
                .build_int_compare(
                    inkwell::IntPredicate::SLE,
                    left_val.into_int_value(),
                    right_val.into_int_value(),
                    "le",
                )?
                .into(),
            BinaryOp::Ge => self
                .builder
                .build_int_compare(
                    inkwell::IntPredicate::SGE,
                    left_val.into_int_value(),
                    right_val.into_int_value(),
                    "ge",
                )?
                .into(),
            BinaryOp::And | BinaryOp::Or => {
                // Logical AND/OR - simplify to i64 for now
                let zero = i64_type.const_int(0, false);
                let lhs_nonzero = self.builder.build_int_compare(
                    inkwell::IntPredicate::NE,
                    left_val.into_int_value(),
                    zero,
                    "lhs_nonzero",
                )?;
                let rhs_nonzero = self.builder.build_int_compare(
                    inkwell::IntPredicate::NE,
                    right_val.into_int_value(),
                    zero,
                    "rhs_nonzero",
                )?;

                if op == BinaryOp::And {
                    self.builder
                        .build_select(lhs_nonzero, rhs_nonzero, zero, "and_result")?
                        .into()
                } else {
                    self.builder
                        .build_select(
                            lhs_nonzero,
                            i64_type.const_int(1, false),
                            rhs_nonzero,
                            "or_result",
                        )?
                        .into()
                }
            }
            BinaryOp::Range => todo!("Codegen for Range operator not implemented"),
        };

        Ok(result)
    }

    /// Generate unary operation
    fn generate_unary_op(
        &mut self,
        op: UnaryOp,
        expr: &Expr,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        let val = self.generate_expr(expr)?;

        match op {
            UnaryOp::Neg => {
                let i64_type = self.context.i64_type();
                let zero = i64_type.const_int(0, false);
                let result = self
                    .builder
                    .build_int_sub(zero, val.into_int_value(), "neg")?;
                Ok(result.into())
            }
            UnaryOp::Pos => {
                // +x is just x
                Ok(val)
            }
            UnaryOp::Not => {
                let i64_type = self.context.i64_type();
                let zero = i64_type.const_int(0, false);
                let result = self.builder.build_int_compare(
                    inkwell::IntPredicate::EQ,
                    val.into_int_value(),
                    zero,
                    "not",
                )?;
                Ok(result.into())
            }
        }
    }

    /// Generate function call
    fn generate_call(
        &mut self,
        name: &str,
        namespace: Option<&str>,
        args: &[Expr],
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        // Handle std library function calls
        if let Some(ns) = namespace {
            // Resolve alias to actual package name
            let actual_package = self
                .imported_packages
                .get(ns)
                .map(|s| s.as_str())
                .unwrap_or(ns);

            // Only allow io.println if io was explicitly imported
            if actual_package == "io" && name == "println" {
                // Check if io was imported
                if !self.imported_packages.contains_key(ns) {
                    return Err(format!(
                        "Package 'io' not imported. Use 'import \"io\"' to import it."
                    )
                    .into());
                }
                return self.generate_io_println(args);
            }

            // Try to find the function in stdlib
            if let Some(fn_def) = self.stdlib.get_function(actual_package, name) {
                // For now, just return a dummy value
                return Ok(self.context.i64_type().const_int(0, false).into());
            }
        }

        let function = self
            .module
            .get_function(name)
            .ok_or(format!("Function not found: {}", name))?;

        // If no arguments, return a dummy value
        if args.is_empty() {
            return Ok(self.context.i64_type().const_int(0, false).into());
        }

        // Just use the first argument as return value for now
        // (proper return handling would require tracking function return types)
        let result = self.generate_expr(&args[0])?;
        Ok(result)
    }

    /// Generate io.println function call
    fn generate_io_println(&mut self, args: &[Expr]) -> CodegenResult<BasicValueEnum<'ctx>> {
        // Get printf function from libc
        let printf_type = self.context.i64_type().fn_type(
            &[self
                .context
                .ptr_type(inkwell::AddressSpace::default())
                .into()],
            true,
        );
        let printf = self.module.add_function(
            "printf",
            printf_type,
            Some(inkwell::module::Linkage::External),
        );

        // Generate the string argument
        if args.is_empty() {
            // Print empty line
            let empty_str = unsafe { self.builder.build_global_string("\n", "empty") }?;
            self.builder
                .build_call(printf, &[empty_str.as_basic_value_enum().into()], "")?;
        } else {
            // Generate first argument as string
            let arg = self.generate_expr(&args[0])?;
            // Cast BasicValueEnum to BasicMetadataValueEnum for function call
            let metadata_arg: inkwell::values::BasicMetadataValueEnum<'_> = arg.into();
            self.builder.build_call(printf, &[metadata_arg], "")?;
        }

        Ok(self.context.i64_type().const_int(0, false).into())
    }

    /// Generate if statement
    fn generate_if(
        &mut self,
        condition: &Expr,
        capture: &Option<String>,
        then_branch: &Stmt,
        else_branch: Option<&Stmt>,
    ) -> CodegenResult<()> {
        let function = self.current_function.unwrap();

        let cond_val = self.generate_expr(condition)?;

        // Check if it's an optional type: struct { value, is_valid }
        let is_valid = if let BasicValueEnum::StructValue(sv) = cond_val {
            if sv.get_type().get_field_types().len() == 2 {
                self.builder
                    .build_extract_value(sv, 1, "is_valid")?
                    .into_int_value()
            } else {
                let zero = self.context.i64_type().const_int(0, false);
                self.builder.build_int_compare(
                    inkwell::IntPredicate::NE,
                    cond_val.into_int_value(),
                    zero,
                    "cond",
                )?
            }
        } else {
            let zero = self.context.i64_type().const_int(0, false);
            self.builder.build_int_compare(
                inkwell::IntPredicate::NE,
                cond_val.into_int_value(),
                zero,
                "cond",
            )?
        };

        let then_block = self.context.append_basic_block(function, "then");
        let else_block = self.context.append_basic_block(function, "else");
        let merge_block = self.context.append_basic_block(function, "ifcont");

        self.builder
            .build_conditional_branch(is_valid, then_block, else_block)?;

        // Then block
        self.builder.position_at_end(then_block);

        // Handle capture
        let mut old_var = None;
        if let Some(name) = capture {
            if let BasicValueEnum::StructValue(sv) = cond_val {
                let val = self.builder.build_extract_value(sv, 0, "captured")?;
                let alloca = self.builder.build_alloca(val.get_type(), name)?;
                self.builder.build_store(alloca, val)?;

                old_var = self.variables.insert(name.clone(), alloca);
                self.variable_types
                    .insert(name.clone(), self.llvm_type_to_lang(&val.get_type()));
            }
        }

        self.generate_stmt(then_branch)?;
        self.builder.build_unconditional_branch(merge_block)?;

        // Restore variable if shadowed
        if let Some(name) = capture {
            if let Some(old) = old_var {
                self.variables.insert(name.clone(), old);
            } else {
                self.variables.remove(name);
            }
        }

        // Else block
        self.builder.position_at_end(else_block);
        if let Some(else_stmt) = else_branch {
            self.generate_stmt(else_stmt)?;
        }
        self.builder.build_unconditional_branch(merge_block)?;

        // Merge block
        self.builder.position_at_end(merge_block);

        Ok(())
    }

    /// Generate if expression
    fn generate_expr_if(
        &mut self,
        condition: &Expr,
        capture: &Option<String>,
        then_branch: &Expr,
        else_branch: &Expr,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        let function = self.current_function.unwrap();

        let cond_val = self.generate_expr(condition)?;

        // Check if it's an optional type: struct { value, is_valid }
        let is_valid = if let BasicValueEnum::StructValue(sv) = cond_val {
            if sv.get_type().get_field_types().len() == 2 {
                self.builder
                    .build_extract_value(sv, 1, "is_valid")?
                    .into_int_value()
            } else {
                let zero = self.context.i64_type().const_int(0, false);
                self.builder.build_int_compare(
                    inkwell::IntPredicate::NE,
                    cond_val.into_int_value(),
                    zero,
                    "cond",
                )?
            }
        } else {
            let zero = self.context.i64_type().const_int(0, false);
            self.builder.build_int_compare(
                inkwell::IntPredicate::NE,
                cond_val.into_int_value(),
                zero,
                "cond",
            )?
        };

        let then_block = self.context.append_basic_block(function, "then");
        let else_block = self.context.append_basic_block(function, "else");
        let merge_block = self.context.append_basic_block(function, "ifcont");

        self.builder
            .build_conditional_branch(is_valid, then_block, else_block)?;

        // Then block
        self.builder.position_at_end(then_block);

        // Handle capture
        let mut old_var = None;
        if let Some(name) = capture {
            if let BasicValueEnum::StructValue(sv) = cond_val {
                let val = self.builder.build_extract_value(sv, 0, "captured")?;
                let alloca = self.builder.build_alloca(val.get_type(), name)?;
                self.builder.build_store(alloca, val)?;

                old_var = self.variables.insert(name.clone(), alloca);
                self.variable_types
                    .insert(name.clone(), self.llvm_type_to_lang(&val.get_type()));
            }
        }

        let then_val = self.generate_expr(then_branch)?;
        let then_actual_block = self.builder.get_insert_block().unwrap();
        self.builder.build_unconditional_branch(merge_block)?;

        // Restore variable if shadowed
        if let Some(name) = capture {
            if let Some(old) = old_var {
                self.variables.insert(name.clone(), old);
            } else {
                self.variables.remove(name);
            }
        }

        // Else block
        self.builder.position_at_end(else_block);
        let else_val = self.generate_expr(else_branch)?;
        let else_actual_block = self.builder.get_insert_block().unwrap();
        self.builder.build_unconditional_branch(merge_block)?;

        // Merge block
        self.builder.position_at_end(merge_block);

        // PHI node
        let phi = self.builder.build_phi(then_val.get_type(), "ifphi")?;
        phi.add_incoming(&[
            (&then_val, then_actual_block),
            (&else_val, else_actual_block),
        ]);

        Ok(phi.as_basic_value())
    }

    /// Generate while loop
    fn generate_while(
        &mut self,
        condition: &Expr,
        capture: &Option<String>,
        body: &Stmt,
    ) -> CodegenResult<()> {
        let function = self.current_function.unwrap();

        let cond_block = self.context.append_basic_block(function, "while_cond");
        let body_block = self.context.append_basic_block(function, "while_body");
        let end_block = self.context.append_basic_block(function, "while_end");

        // Jump to condition
        self.builder.build_unconditional_branch(cond_block)?;

        // Condition block
        self.builder.position_at_end(cond_block);
        let cond_val = self.generate_expr(condition)?;

        // Check if it's an optional type: struct { value, is_valid }
        let is_valid = if let BasicValueEnum::StructValue(sv) = cond_val {
            if sv.get_type().get_field_types().len() == 2 {
                self.builder
                    .build_extract_value(sv, 1, "is_valid")?
                    .into_int_value()
            } else {
                let zero = self.context.i64_type().const_int(0, false);
                self.builder.build_int_compare(
                    inkwell::IntPredicate::NE,
                    cond_val.into_int_value(),
                    zero,
                    "cond",
                )?
            }
        } else {
            let zero = self.context.i64_type().const_int(0, false);
            self.builder.build_int_compare(
                inkwell::IntPredicate::NE,
                cond_val.into_int_value(),
                zero,
                "cond",
            )?
        };

        self.builder
            .build_conditional_branch(is_valid, body_block, end_block)?;

        // Body block
        self.builder.position_at_end(body_block);

        // Handle capture
        let mut old_var = None;
        if let Some(name) = capture {
            if let BasicValueEnum::StructValue(sv) = cond_val {
                let val = self.builder.build_extract_value(sv, 0, "captured")?;
                let alloca = self.builder.build_alloca(val.get_type(), name)?;
                self.builder.build_store(alloca, val)?;

                old_var = self.variables.insert(name.clone(), alloca);
                self.variable_types
                    .insert(name.clone(), self.llvm_type_to_lang(&val.get_type()));
            }
        }

        self.generate_stmt(body)?;
        self.builder.build_unconditional_branch(cond_block)?;

        // Restore variable if shadowed
        if let Some(name) = capture {
            if let Some(old) = old_var {
                self.variables.insert(name.clone(), old);
            } else {
                self.variables.remove(name);
            }
        }

        // End block
        self.builder.position_at_end(end_block);

        Ok(())
    }

    /// Generate infinite loop
    fn generate_loop(&mut self, body: &Stmt) -> CodegenResult<()> {
        let function = self.current_function.unwrap();

        let body_block = self.context.append_basic_block(function, "loop_body");
        let end_block = self.context.append_basic_block(function, "loop_end");

        // Jump to body
        self.builder.build_unconditional_branch(body_block)?;

        // Body block
        self.builder.position_at_end(body_block);
        self.generate_stmt(body)?;
        self.builder.build_unconditional_branch(body_block)?;

        // End block
        self.builder.position_at_end(end_block);

        Ok(())
    }

    /// Convert our Type to LLVM type
    fn llvm_type(&self, ty: &Type) -> BasicTypeEnum<'ctx> {
        match ty {
            Type::I8 => self.context.i8_type().into(),
            Type::I16 => self.context.i16_type().into(),
            Type::I32 => self.context.i32_type().into(),
            Type::I64 => self.context.i64_type().into(),
            Type::U8 => self.context.i8_type().into(),
            Type::U16 => self.context.i16_type().into(),
            Type::U32 => self.context.i32_type().into(),
            Type::U64 => self.context.i64_type().into(),
            Type::Bool => self.context.bool_type().into(),
            Type::SelfType => self.context.i64_type().into(), // TODO: Resolve to actual struct type
            Type::Pointer(_) => self.context.i64_type().into(), // TODO: Implement pointer types
            Type::Option(inner) => {
                // Optional type: represented as a struct { value, is_valid }
                // where is_valid is a boolean indicating whether the value is present
                let bool_type = self.context.bool_type();

                // Use the appropriate value type based on the inner type
                let value_type: BasicTypeEnum<'ctx> = match inner.as_ref() {
                    Type::I8 => self.context.i8_type().into(),
                    Type::I16 => self.context.i16_type().into(),
                    Type::I32 => self.context.i32_type().into(),
                    Type::I64 => self.context.i64_type().into(),
                    Type::U8 => self.context.i8_type().into(),
                    Type::U16 => self.context.i16_type().into(),
                    Type::U32 => self.context.i32_type().into(),
                    Type::U64 => self.context.i64_type().into(),
                    Type::Bool => self.context.bool_type().into(),
                    _ => self.context.i64_type().into(), // Default for custom/generic types
                };

                self.context
                    .struct_type(&[value_type.into(), bool_type.into()], false)
                    .into()
            }
            Type::Tuple(types) => {
                // Tuple type: represented as a struct with all elements
                let mut element_types: Vec<BasicTypeEnum<'ctx>> = Vec::new();
                for t in types {
                    element_types.push(self.llvm_type(t));
                }
                self.context.struct_type(&element_types, false).into()
            }
            Type::Void | Type::Custom { .. } | Type::GenericParam(_) | Type::Array { .. } => {
                // For void, custom types, generics, and arrays, we'll just use i64 to avoid the conversion issue
                self.context.i64_type().into()
            }
        }
    }

    /// Convert LLVM type to our Type (simplified version)
    fn llvm_type_to_lang(&self, ty: &BasicTypeEnum<'ctx>) -> Type {
        match ty {
            BasicTypeEnum::IntType(it) => match it.get_bit_width() {
                8 => Type::I8,
                16 => Type::I16,
                32 => Type::I32,
                64 => Type::I64,
                _ => Type::I64,
            },
            BasicTypeEnum::FloatType(_) => Type::I64, // Default float to i64
            BasicTypeEnum::PointerType(_) => Type::I64, // Default pointer to i64
            BasicTypeEnum::StructType(_) => {
                // For structs, create a placeholder tuple - the actual type
                // should be specified explicitly or inferred from the expression
                Type::Tuple(vec![Type::I64, Type::I64])
            }
            BasicTypeEnum::ArrayType(_) => Type::I64, // Default array to i64
            BasicTypeEnum::VectorType(_) => Type::I64, // Default vector to i64
            BasicTypeEnum::ScalableVectorType(_) => Type::I64, // Default scalable vector to i64
        }
    }

    /// Print the generated LLVM IR
    pub fn print_ir(&self) -> String {
        self.module.print_to_string().to_string()
    }
}
