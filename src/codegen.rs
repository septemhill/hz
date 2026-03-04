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

    /// Generate code for a program
    pub fn generate(&mut self, program: &Program) -> CodegenResult<()> {
        // First, process all import statements (for duplicate checking)
        for (alias, package_name) in &program.imports {
            let namespace = alias.as_deref().unwrap_or(package_name.as_str());

            // Check for duplicate import
            if self.imported_packages.contains_key(namespace) {
                return Err(
                    format!("Duplicate import: '{}' is already imported", namespace).into(),
                );
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

        // Then, declare all structs
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
        // For main function, default to i64; for other functions, default to void
        let default_return_type = if fn_def.name == "main" {
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
