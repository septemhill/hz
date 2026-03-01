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

    // Return type of current function
    return_type: Option<Type>,
}

/// Result of code generation
pub type CodegenResult<T> = Result<T, Box<dyn Error>>;

impl<'ctx> CodeGenerator<'ctx> {
    /// Create a new code generator
    pub fn new(context: &'ctx Context, module_name: &str) -> CodegenResult<Self> {
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
            return_type: None,
        })
    }

    /// Generate code for a program
    pub fn generate(&mut self, program: &Program) -> CodegenResult<()> {
        // First, declare all structs
        for struct_def in &program.structs {
            self.declare_struct(struct_def)?;
        }

        // Then, declare all enums
        for enum_def in &program.enums {
            self.declare_enum(enum_def)?;
        }

        // Then, declare all functions
        for fn_def in &program.functions {
            self.declare_function(fn_def)?;
        }

        // Generate code for each function
        for fn_def in &program.functions {
            self.generate_function(fn_def)?;
        }

        Ok(())
    }

    /// Declare a struct type in LLVM
    fn declare_struct(&mut self, struct_def: &StructDef) -> CodegenResult<()> {
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
    fn declare_enum(&mut self, enum_def: &EnumDef) -> CodegenResult<()> {
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
    fn declare_function(&mut self, fn_def: &FnDef) -> CodegenResult<()> {
        let return_type = self.llvm_type(fn_def.return_ty.as_ref().unwrap_or(&Type::I64));
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

        // If the function doesn't have a return statement and returns void, add implicit return
        if fn_def.return_ty == Some(Type::Void) || fn_def.return_ty.is_none() {
            self.builder.build_return(None)?;
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
            Stmt::Let {
                mutable: _,
                name,
                ty,
                value,
                visibility: _,
                span: _,
            } => {
                let var_type = self.llvm_type(ty.as_ref().unwrap_or(&Type::I64));
                let alloca = self.builder.build_alloca(var_type, name)?;

                if let Some(val) = value {
                    let llvm_val = self.generate_expr(val)?;
                    self.builder.build_store(alloca, llvm_val)?;
                }

                self.variables.insert(name.clone(), alloca);
                Ok(())
            }
            Stmt::Assign {
                target,
                op,
                value,
                span: _,
            } => {
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
                then_branch,
                else_branch,
                ..
            } => self.generate_if(condition, then_branch, else_branch.as_deref()),
            Stmt::While {
                condition, body, ..
            } => self.generate_while(condition, body),
            Stmt::Loop { body, .. } => self.generate_loop(body),
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
            Expr::Ident(name, _) => {
                let ptr = self
                    .variables
                    .get(name)
                    .ok_or(format!("Variable not found: {}", name))?;
                let load = self
                    .builder
                    .build_load(self.llvm_type(&Type::I64), *ptr, name)?;
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
            if ns == "io" && name == "println" {
                return self.generate_io_println(args);
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
                .i8_type()
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
        then_branch: &Stmt,
        else_branch: Option<&Stmt>,
    ) -> CodegenResult<()> {
        let function = self.current_function.unwrap();

        let cond_val = self.generate_expr(condition)?;
        let zero = self.context.i64_type().const_int(0, false);
        let cond_nonzero = self.builder.build_int_compare(
            inkwell::IntPredicate::NE,
            cond_val.into_int_value(),
            zero,
            "cond",
        )?;

        let then_block = self.context.append_basic_block(function, "then");
        let else_block = self.context.append_basic_block(function, "else");
        let merge_block = self.context.append_basic_block(function, "ifcont");

        self.builder
            .build_conditional_branch(cond_nonzero, then_block, else_block)?;

        // Then block
        self.builder.position_at_end(then_block);
        self.generate_stmt(then_branch)?;
        self.builder.build_unconditional_branch(merge_block)?;

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

    /// Generate while loop
    fn generate_while(&mut self, condition: &Expr, body: &Stmt) -> CodegenResult<()> {
        let function = self.current_function.unwrap();

        let cond_block = self.context.append_basic_block(function, "while_cond");
        let body_block = self.context.append_basic_block(function, "while_body");
        let end_block = self.context.append_basic_block(function, "while_end");

        // Jump to condition
        self.builder.build_unconditional_branch(cond_block)?;

        // Condition block
        self.builder.position_at_end(cond_block);
        let cond_val = self.generate_expr(condition)?;
        let zero = self.context.i64_type().const_int(0, false);
        let cond_nonzero = self.builder.build_int_compare(
            inkwell::IntPredicate::NE,
            cond_val.into_int_value(),
            zero,
            "while_cond",
        )?;
        self.builder
            .build_conditional_branch(cond_nonzero, body_block, end_block)?;

        // Body block
        self.builder.position_at_end(body_block);
        self.generate_stmt(body)?;
        self.builder.build_unconditional_branch(cond_block)?;

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
            Type::Void | Type::Custom { .. } | Type::GenericParam(_) => {
                // For void, custom types, and generics, we'll just use i64 to avoid the conversion issue
                self.context.i64_type().into()
            }
        }
    }

    /// Print the generated LLVM IR
    pub fn print_ir(&self) -> String {
        self.module.print_to_string().to_string()
    }
}
