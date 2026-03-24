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
    BasicMetadataValueEnum, BasicValue, BasicValueEnum, FunctionValue, GlobalValue, PointerValue,
};

use crate::ast::*;
use crate::hir;
use crate::sema::infer::{TypedFnDef, TypedStructDef};
use crate::stdlib::StdLib;

/// Code generator context
#[allow(unused)]
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

    // Defer stack - tracks deferred statements per scope (LIFO)
    // Each Vec<HirStmt> represents defers in one scope
    defer_stack: Vec<Vec<hir::HirStmt>>,

    // Defer! stack - tracks deferred! statements per scope (LIFO)
    // These only execute when an error occurs in a try statement
    defer_bang_stack: Vec<Vec<hir::HirStmt>>,

    // Return type of current function
    return_type: Option<Type>,

    // Stack of loop end blocks for break statements
    // Each entry is a vector of (end_block, label) pairs
    loop_end_blocks: Vec<Vec<(inkwell::basic_block::BasicBlock<'ctx>, Option<String>)>>,

    // Standard library
    stdlib: StdLib,

    // Track imported packages (for duplicate checking)
    pub imported_packages: HashMap<String, String>, // alias -> package_name

    // Current module name for mangling
    module_name: String,

    // Enum variants (enum_name -> variant_name -> variant_index)
    enum_variants: HashMap<String, HashMap<String, u32>>,

    // Struct field indices (struct_name -> field_name -> field_index)
    struct_field_indices: HashMap<String, HashMap<String, u32>>,

    // Struct, Enum, and Error definitions for type lookup
    pub structs: HashMap<String, TypedStructDef>,
    pub enums: HashMap<String, EnumDef>,
    pub errors: HashMap<String, ErrorDef>,
}

/// Result of code generation
pub type CodegenResult<T> = Result<T, Box<dyn Error>>;

#[derive(Clone, Copy)]
enum PrintfArgKind {
    String,
    Integer,
    Float,
}

#[allow(unused)]
impl<'ctx> CodeGenerator<'ctx> {
    #[allow(unused)]
    /// Create a new code generator
    pub fn new(
        context: &'ctx Context,
        module_name: &str,
        stdlib: StdLib,
        structs: HashMap<String, TypedStructDef>,
        enums: HashMap<String, EnumDef>,
        errors: HashMap<String, ErrorDef>,
    ) -> CodegenResult<Self> {
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
            defer_stack: Vec::new(),
            defer_bang_stack: Vec::new(),
            return_type: None,
            loop_end_blocks: Vec::new(),
            stdlib,
            imported_packages: HashMap::new(),
            module_name: module_name.to_string(),
            enum_variants: HashMap::new(),
            struct_field_indices: HashMap::new(),
            structs,
            enums,
            errors,
        })
    }

    /// Push a new scope onto the defer stack
    fn push_defer_scope(&mut self) {
        self.defer_stack.push(Vec::new());
    }

    /// Pop the current scope and execute all defers in LIFO order
    fn pop_defer_scope(&mut self) -> CodegenResult<()> {
        if let Some(defers) = self.defer_stack.pop() {
            // Execute defers in reverse order (LIFO)
            // We iterate in reverse but since we want the defers to appear after
            // all other statements in the block, we need to handle the builder position
            for defer_stmt in defers.iter().rev() {
                self.generate_hir_stmt(defer_stmt)?;
            }
        }
        Ok(())
    }

    /// Add a defer statement to the current scope
    fn add_defer(&mut self, stmt: hir::HirStmt) {
        if let Some(current_scope) = self.defer_stack.last_mut() {
            current_scope.push(stmt);
        }
    }

    /// Push a new scope onto the defer! stack
    fn push_defer_bang_scope(&mut self) {
        self.defer_bang_stack.push(Vec::new());
    }

    /// Pop the current scope and execute all defer!s in LIFO order (only on error)
    fn pop_defer_bang_scope(&mut self) -> CodegenResult<()> {
        if let Some(defers) = self.defer_bang_stack.pop() {
            // Execute defer!s in reverse order (LIFO)
            for defer_stmt in defers.iter().rev() {
                self.generate_hir_stmt(defer_stmt)?;
            }
        }
        Ok(())
    }

    /// Add a defer! statement to the current scope
    fn add_defer_bang(&mut self, stmt: hir::HirStmt) {
        if let Some(current_scope) = self.defer_bang_stack.last_mut() {
            current_scope.push(stmt);
        }
    }

    /// Execute defer! statements (cleanup on error) - called when an error is detected
    fn execute_defer_bang_on_error(&mut self) -> CodegenResult<()> {
        // Execute all defer! statements in the current scope
        // This is called when a try statement returns an error
        if let Some(defers) = self.defer_bang_stack.last() {
            let defers_to_run: Vec<hir::HirStmt> = defers.iter().rev().cloned().collect();
            for defer_stmt in defers_to_run {
                self.generate_hir_stmt(&defer_stmt)?;
            }
        }
        Ok(())
    }

    /// Generate code for an HIR program
    pub fn generate_hir(&mut self, program: &hir::HirProgram) -> CodegenResult<()> {
        // Generate code for each function in HIR
        for hir_fn in &program.functions {
            self.generate_hir_function(hir_fn)?;
        }

        Ok(())
    }

    /// Convert a condition value to i1 (boolean) for branching
    /// If already i1, returns as-is; otherwise converts i64 to i1
    /// Handles optional types (represented as struct { value, valid_flag })
    fn condition_to_i1(
        &mut self,
        cond_val: BasicValueEnum<'ctx>,
        name: &str,
    ) -> CodegenResult<inkwell::values::IntValue<'ctx>> {
        match cond_val {
            BasicValueEnum::IntValue(iv) if iv.get_type().get_bit_width() == 1 => Ok(iv),
            BasicValueEnum::StructValue(sv) => {
                // Optional types are represented as struct { value, valid_flag }
                // Extract the valid flag (second element, index 1)
                let valid_flag = self.builder.build_extract_value(sv, 1, name)?;
                Ok(valid_flag.into_int_value())
            }
            _ => {
                let zero = self.context.i64_type().const_int(0, false);
                let result = self.builder.build_int_compare(
                    inkwell::IntPredicate::NE,
                    cond_val.into_int_value(),
                    zero,
                    name,
                );
                result.map_err(|e| Box::new(e) as Box<dyn Error>)
            }
        }
    }

    /// Mangle a function name based on module context
    fn mangle_name(&self, name: &str, is_main: bool) -> String {
        if is_main || name == "main" {
            "main".to_string()
        } else {
            format!("{}_{}", self.module_name, name)
        }
    }

    fn llvm_function_return_type(
        &self,
        return_ty: &Type,
        is_main: bool,
    ) -> Option<BasicTypeEnum<'ctx>> {
        if is_main {
            Some(self.context.i64_type().into())
        } else if return_ty == &Type::Void {
            None
        } else {
            Some(self.llvm_type(return_ty))
        }
    }

    fn default_llvm_return_value(
        &self,
        return_ty: &Type,
        is_main: bool,
    ) -> Option<BasicValueEnum<'ctx>> {
        match self.llvm_function_return_type(return_ty, is_main) {
            Some(llvm_ty) if is_main => Some(self.context.i64_type().const_int(0, false).into()),
            Some(llvm_ty) => Some(llvm_ty.const_zero()),
            None => None,
        }
    }

    fn current_function_is_main(&self) -> bool {
        self.current_function
            .map(|function| function.get_name().to_str().ok() == Some("main"))
            .unwrap_or(false)
    }

    fn current_block_has_terminator(&self) -> bool {
        self.builder
            .get_insert_block()
            .and_then(|block| block.get_terminator())
            .is_some()
    }

    fn get_or_create_printf(&self) -> FunctionValue<'ctx> {
        let printf_type = self.context.i64_type().fn_type(
            &[self
                .context
                .ptr_type(inkwell::AddressSpace::default())
                .into()],
            true,
        );

        self.module.get_function("printf").unwrap_or_else(|| {
            self.module.add_function(
                "printf",
                printf_type,
                Some(inkwell::module::Linkage::External),
            )
        })
    }

    fn get_or_create_last_error_global(&self) -> GlobalValue<'ctx> {
        self.module
            .get_global("__lang_last_error_message")
            .unwrap_or_else(|| {
                let ptr_ty = self.context.ptr_type(inkwell::AddressSpace::default());
                let global = self
                    .module
                    .add_global(ptr_ty, None, "__lang_last_error_message");
                global.set_initializer(&ptr_ty.const_null());
                global
            })
    }

    fn clear_last_error_message(&mut self) -> CodegenResult<()> {
        let ptr_ty = self.context.ptr_type(inkwell::AddressSpace::default());
        let last_error = self.get_or_create_last_error_global();
        self.builder
            .build_store(last_error.as_pointer_value(), ptr_ty.const_null())?;
        Ok(())
    }

    fn set_last_error_message(&mut self, message: &str) -> CodegenResult<()> {
        let last_error = self.get_or_create_last_error_global();
        let message_ptr = unsafe {
            self.builder
                .build_global_string(message, "last_error_message")
        }?;
        self.builder.build_store(
            last_error.as_pointer_value(),
            message_ptr.as_pointer_value(),
        )?;
        Ok(())
    }

    fn emit_last_error_message(&mut self) -> CodegenResult<()> {
        let function = self.current_function.ok_or("No current function")?;
        let ptr_ty = self.context.ptr_type(inkwell::AddressSpace::default());
        let last_error = self.get_or_create_last_error_global();
        let last_error_ptr = self
            .builder
            .build_load(
                ptr_ty,
                last_error.as_pointer_value(),
                "last_error_message_ptr",
            )?
            .into_pointer_value();

        let has_message = self
            .builder
            .build_is_not_null(last_error_ptr, "has_last_error_message")?;

        let print_block = self
            .context
            .append_basic_block(function, "main_error_print");
        let fallback_block = self
            .context
            .append_basic_block(function, "main_error_print_fallback");
        let merge_block = self
            .context
            .append_basic_block(function, "main_error_print_done");

        self.builder
            .build_conditional_branch(has_message, print_block, fallback_block)?;

        self.builder.position_at_end(print_block);
        let printf = self.get_or_create_printf();
        let string_fmt = unsafe { self.builder.build_global_string("%s\n", "main_error_fmt") }?;
        self.builder.build_call(
            printf,
            &[
                string_fmt.as_basic_value_enum().into(),
                last_error_ptr.as_basic_value_enum().into(),
            ],
            "",
        )?;
        self.builder.build_unconditional_branch(merge_block)?;

        self.builder.position_at_end(fallback_block);
        let fallback = unsafe { self.builder.build_global_string("error\n", "main_error") }?;
        self.builder
            .build_call(printf, &[fallback.as_basic_value_enum().into()], "")?;
        self.builder.build_unconditional_branch(merge_block)?;

        self.builder.position_at_end(merge_block);
        Ok(())
    }

    fn emit_main_error_exit(&mut self) -> CodegenResult<()> {
        self.emit_last_error_message()?;
        self.builder
            .build_return(Some(&self.context.i64_type().const_int(1, false)))?;
        Ok(())
    }

    fn hir_expr_type<'a>(&self, expr: &'a hir::HirExpr) -> &'a Type {
        match expr {
            hir::HirExpr::Int(_, ty, _)
            | hir::HirExpr::Float(_, ty, _)
            | hir::HirExpr::Bool(_, ty, _)
            | hir::HirExpr::String(_, ty, _)
            | hir::HirExpr::Char(_, ty, _)
            | hir::HirExpr::Null(ty, _)
            | hir::HirExpr::Ident(_, ty, _)
            | hir::HirExpr::Tuple { ty, .. }
            | hir::HirExpr::TupleIndex { ty, .. }
            | hir::HirExpr::Array { ty, .. }
            | hir::HirExpr::Binary { ty, .. }
            | hir::HirExpr::Unary { ty, .. }
            | hir::HirExpr::If { ty, .. }
            | hir::HirExpr::Block { ty, .. }
            | hir::HirExpr::MemberAccess { ty, .. }
            | hir::HirExpr::Struct { ty, .. } => ty,
            hir::HirExpr::Call { return_ty, .. } => return_ty,
            hir::HirExpr::Try { expr, .. } => self
                .hir_expr_type(expr)
                .result_inner()
                .unwrap_or_else(|| self.hir_expr_type(expr)),
            hir::HirExpr::Catch { expr, .. } => self.hir_expr_type(expr),
        }
    }

    fn build_typed_int_constant(&self, value: i64, ty: &Type) -> BasicValueEnum<'ctx> {
        match self.llvm_type(ty) {
            BasicTypeEnum::IntType(int_ty) => int_ty
                .const_int(value as u64, ty.is_signed_integer())
                .into(),
            _ => self.context.i64_type().const_int(value as u64, true).into(),
        }
    }

    fn promote_printf_arg(
        &self,
        value: BasicValueEnum<'ctx>,
        kind: PrintfArgKind,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        match kind {
            PrintfArgKind::String => Ok(value),
            PrintfArgKind::Integer => {
                if value.is_int_value() {
                    let int_val = value.into_int_value();
                    if int_val.get_type().get_bit_width() == 64 {
                        Ok(value)
                    } else if int_val.get_type().get_bit_width() == 1 {
                        Ok(self
                            .builder
                            .build_int_z_extend(int_val, self.context.i64_type(), "printf_bool")?
                            .into())
                    } else {
                        Ok(self
                            .builder
                            .build_int_s_extend(int_val, self.context.i64_type(), "printf_int")?
                            .into())
                    }
                } else {
                    Ok(value)
                }
            }
            PrintfArgKind::Float => {
                if value.is_float_value() {
                    let float_val = value.into_float_value();
                    if float_val.get_type().get_bit_width() == 64 {
                        Ok(value)
                    } else {
                        Ok(self
                            .builder
                            .build_float_cast(float_val, self.context.f64_type(), "printf_float")?
                            .into())
                    }
                } else {
                    Ok(value)
                }
            }
        }
    }

    /// Build function type from return type and parameter types
    fn build_function_type(
        &self,
        return_ty: &Type,
        param_tys: &[Type],
        is_main: bool,
    ) -> inkwell::types::FunctionType<'ctx> {
        let param_types: Vec<BasicMetadataTypeEnum> =
            param_tys.iter().map(|p| self.llvm_type(p).into()).collect();

        match self.llvm_function_return_type(return_ty, is_main) {
            Some(return_type) => return_type.fn_type(&param_types, false),
            None => self.context.void_type().fn_type(&param_types, false),
        }
    }

    /// Generate code for an HIR function
    fn generate_hir_function(&mut self, hir_fn: &hir::HirFn) -> CodegenResult<()> {
        let is_main = hir_fn.name == "main";
        let mangled_name = self.mangle_name(&hir_fn.name, is_main);

        let function = self.module.get_function(&mangled_name).ok_or(format!(
            "Function not declared: {} (original: {})",
            mangled_name, hir_fn.name
        ))?;

        self.current_function = Some(function);
        self.return_type = Some(hir_fn.return_ty.clone());

        self.variables.clear();
        self.variable_types.clear();
        self.const_variables.clear();
        self.defer_stack.clear();
        self.defer_bang_stack.clear();

        let entry_block = self.context.append_basic_block(function, "entry");
        self.builder.position_at_end(entry_block);
        self.current_block = Some(entry_block);

        // Push function scope for defers
        self.push_defer_scope();
        self.push_defer_bang_scope();

        if is_main {
            self.clear_last_error_message()?;
        }

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

        let returns_llvm_void = !is_main && hir_fn.return_ty == Type::Void;
        let mut returned_early = false;

        // Track statements
        let stmt_count = hir_fn.body.len();
        for (i, stmt) in hir_fn.body.iter().enumerate() {
            if self.current_block_has_terminator() {
                break;
            }

            // Check if this is the last statement in a void function
            let is_last = i == stmt_count - 1;

            match stmt {
                hir::HirStmt::Return(_, _) => {
                    self.generate_hir_stmt(stmt)?;
                    returned_early = true;
                    break;
                }
                _ if is_last && returns_llvm_void => {
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
                    // Execute defers AFTER the last statement
                    self.pop_defer_scope()?;
                    self.builder.build_return(None)?;
                    returned_early = true;
                    break;
                }
                _ if is_last && !returns_llvm_void => {
                    // Last statement in non-void function - use value as return
                    self.generate_hir_stmt(stmt)?;
                    // Execute defers AFTER the last statement
                    self.pop_defer_scope()?;
                    if let Some(default_ret) =
                        self.default_llvm_return_value(&hir_fn.return_ty, is_main)
                    {
                        self.builder.build_return(Some(&default_ret))?;
                    } else {
                        self.builder.build_return(None)?;
                    }
                    returned_early = true;
                    break;
                }
                _ => {
                    self.generate_hir_stmt(stmt)?;
                }
            }
        }

        if !returned_early && !self.current_block_has_terminator() {
            self.pop_defer_scope()?;
            if let Some(default_ret) = self.default_llvm_return_value(&hir_fn.return_ty, is_main) {
                self.builder.build_return(Some(&default_ret))?;
            } else {
                self.builder.build_return(None)?;
            }
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
                    // Check if we need special handling for identifiers that might be functions
                    let mut llvm_val = if let hir::HirExpr::Ident(ident_name, _, _) = val {
                        // Try to look up as a function first
                        let fn_name = if ident_name == "main" {
                            "main".to_string()
                        } else {
                            format!("{}_{}", self.module_name, ident_name)
                        };

                        if self.module.get_function(&fn_name).is_some() {
                            eprintln!("DEBUG: Let - identifier is a function: {}", fn_name);
                            // This identifier is a function - get the function pointer
                            if let Some(fn_val) = self.module.get_function(&fn_name) {
                                fn_val.as_global_value().as_pointer_value().into()
                            } else {
                                self.generate_hir_expr(val)?
                            }
                        } else {
                            // Not a function, process normally
                            self.generate_hir_expr(val)?
                        }
                    } else {
                        self.generate_hir_expr(val)?
                    };
                    llvm_val = self.coerce_type(llvm_val, ty)?;
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
                // Skip underscore assignment (used for ignoring values)
                if target == "_" {
                    let _llvm_val = self.generate_hir_expr(value)?;
                    // Just evaluate and discard
                    return Ok(());
                }

                if target.contains('.') {
                    let parts: Vec<&str> = target.split('.').collect();
                    let obj_name = parts[0];
                    let member = parts[1];

                    let ptr = self.variables.get(obj_name).ok_or("Var not found")?.clone();
                    let var_ty = self.variable_types.get(obj_name).unwrap();

                    // First, get the struct name from the variable type
                    let struct_name = match var_ty {
                        Type::Pointer(inner) => {
                            if let Type::Custom { name, .. } = &**inner {
                                name.clone()
                            } else {
                                return Err("Member assignment on non-custom pointer type".into());
                            }
                        }
                        Type::Custom { name, .. } => name.clone(),
                        _ => return Err("Member assignment on non-struct type".into()),
                    };

                    // Look up the field index from our mapping
                    let field_idx = self
                        .struct_field_indices
                        .get(&struct_name)
                        .and_then(|fields| fields.get(member))
                        .copied()
                        .ok_or_else(|| {
                            format!("Field '{}' not found in struct '{}'", member, struct_name)
                        })?;

                    // Determine struct_ptr and struct_type based on whether the variable is a pointer to a struct or the struct itself
                    let (struct_ptr, struct_type) = if let Type::Pointer(inner) = var_ty {
                        if let Type::Custom {
                            name: struct_name, ..
                        } = &**inner
                        {
                            let st =
                                self.context
                                    .get_struct_type(struct_name)
                                    .unwrap_or_else(|| {
                                        panic!("Struct lookup failed for: {}", struct_name)
                                    });
                            // The alloca holds a pointer to the struct. Load it using the generic pointer type.
                            let opaque_ptr_type =
                                self.context.ptr_type(inkwell::AddressSpace::default());
                            let loaded_ptr = self
                                .builder
                                .build_load(opaque_ptr_type, ptr, "obj_ptr")?
                                .into_pointer_value();
                            (loaded_ptr, st)
                        } else {
                            return Err("Member assignment on non-custom pointer type".into());
                        }
                    } else if let Type::Custom {
                        name: struct_name, ..
                    } = var_ty
                    {
                        let st = self
                            .context
                            .get_struct_type(struct_name)
                            .unwrap_or_else(|| panic!("Struct lookup failed for: {}", struct_name));
                        // ptr is alloca to struct
                        (ptr, st)
                    } else {
                        return Err("Member assignment on non-struct type".into());
                    };

                    let field_ptr = self
                        .builder
                        .build_struct_gep(struct_type, struct_ptr, field_idx, "field_ptr")
                        .map_err(|e| e.to_string())?;

                    // Get the exact field type to coerce the assigned value
                    let field_type = struct_type.get_field_type_at_index(field_idx).unwrap();
                    let mut llvm_val = self.generate_hir_expr(value)?;

                    // Simple int casting for assignment coercion if needed
                    if field_type.is_int_type() && llvm_val.is_int_value() {
                        llvm_val = self
                            .builder
                            .build_int_cast(
                                llvm_val.into_int_value(),
                                field_type.into_int_type(),
                                "assign_cast",
                            )?
                            .into();
                    }

                    self.builder.build_store(field_ptr, llvm_val)?;
                    return Ok(());
                }

                let ptr = self.variables.get(target).ok_or("Var not found")?.clone();
                let expected_t = self
                    .variable_types
                    .get(target)
                    .ok_or("Var type not found")?
                    .clone();
                let mut llvm_val = self.generate_hir_expr(value)?;
                llvm_val = self.coerce_type(llvm_val, &expected_t)?;
                self.builder.build_store(ptr, llvm_val)?;
                Ok(())
            }
            hir::HirStmt::Return(value, _) => {
                // Execute defers before returning
                self.pop_defer_scope()?;

                let is_main = self.current_function_is_main();
                let ret_ty = self
                    .return_type
                    .clone()
                    .ok_or("Return encountered without current function return type")?;

                if let Some(val) = value {
                    let mut llvm_val = self.generate_hir_expr(val)?;

                    // Skip coercion for error type expressions - they already return i64
                    let is_error_expr = match val {
                        hir::HirExpr::MemberAccess {
                            ty: Type::Error, ..
                        } => true,
                        _ => false,
                    };

                    if is_main && ret_ty.is_result() && is_error_expr {
                        self.emit_main_error_exit()?;
                        return Ok(());
                    }

                    if ret_ty.is_result() && !is_main {
                        if is_error_expr {
                            let result_type = self.llvm_type(&ret_ty).into_struct_type();
                            let flag = self.context.bool_type().const_int(1, false);
                            let result_val = self
                                .builder
                                .build_insert_value(result_type.const_zero(), flag, 1, "ret_error")?
                                .into_struct_value()
                                .as_basic_value_enum();
                            self.builder.build_return(Some(&result_val))?;
                            return Ok(());
                        }

                        if self.hir_expr_type(val) != &ret_ty {
                            llvm_val = self.coerce_type(llvm_val, &ret_ty)?;
                        }
                    } else if !is_error_expr {
                        if is_main {
                            if let BasicValueEnum::IntValue(int_val) = llvm_val {
                                let i64_type = self.context.i64_type();
                                if int_val.get_type() != i64_type {
                                    llvm_val = self
                                        .builder
                                        .build_int_cast(int_val, i64_type, "ret_i64")?
                                        .into();
                                }
                            }
                        } else if self.hir_expr_type(val) != &ret_ty {
                            llvm_val = self.coerce_type(llvm_val, &ret_ty)?;
                        }
                    }

                    self.builder.build_return(Some(&llvm_val))?;
                } else {
                    if let Some(default_ret) = self.default_llvm_return_value(&ret_ty, is_main) {
                        self.builder.build_return(Some(&default_ret))?;
                    } else {
                        self.builder.build_return(None)?;
                    }
                }
                Ok(())
            }
            hir::HirStmt::If {
                condition,
                capture,
                then_branch,
                else_branch,
                ..
            } => {
                let cond_val = self.generate_hir_expr(condition)?;
                let function = self.current_function.unwrap();
                let then_block = self.context.append_basic_block(function, "then");
                let else_block = self.context.append_basic_block(function, "else");
                let merge_block = self.context.append_basic_block(function, "cont");

                // Convert condition to i1 for branching
                let is_true = self.condition_to_i1(cond_val, "is_true")?;
                self.builder
                    .build_conditional_branch(is_true, then_block, else_block)?;

                // Then block - position at then_block BEFORE handling capture
                self.builder.position_at_end(then_block);

                // Handle capture variable if present (e.g., if (opt) |data| { ... })
                // This must be inside the then block
                let mut old_var = None;
                let mut old_ty = None;
                let capture_name = capture.clone();
                if let Some(ref name) = capture_name {
                    if let BasicValueEnum::StructValue(sv) = cond_val {
                        // Extract the value (first element) from the struct
                        let val = self.builder.build_extract_value(sv, 0, "captured")?;
                        let alloca = self.builder.build_alloca(val.get_type(), name)?;
                        self.builder.build_store(alloca, val)?;
                        // Save old variable if it exists
                        old_var = self.variables.insert(name.clone(), alloca);
                        old_ty = self
                            .variable_types
                            .insert(name.clone(), self.llvm_type_to_lang(&val.get_type()));
                    }
                }

                self.generate_hir_stmt(then_branch)?;
                if !self.current_block_has_terminator() {
                    self.builder.build_unconditional_branch(merge_block)?;
                }

                // Restore variable if it was shadowed
                if let Some(ref name) = capture_name {
                    if let Some(old) = old_var {
                        self.variables.insert(name.clone(), old);
                    } else {
                        self.variables.remove(name);
                    }

                    if let Some(old) = old_ty {
                        self.variable_types.insert(name.clone(), old);
                    } else {
                        self.variable_types.remove(name);
                    }
                }

                // Else block
                self.builder.position_at_end(else_block);
                if let Some(eb) = else_branch {
                    self.generate_hir_stmt(eb)?;
                    if !self.current_block_has_terminator() {
                        self.builder.build_unconditional_branch(merge_block)?;
                    }
                } else {
                    self.builder.build_unconditional_branch(merge_block)?;
                }

                // Merge block
                self.builder.position_at_end(merge_block);
                Ok(())
            }
            hir::HirStmt::For {
                label,
                var_name,
                index_var,
                iterable,
                body,
                span: _,
            } => {
                let function = self.current_function.unwrap();

                // Create blocks
                let eval_start_block = self.context.append_basic_block(function, "for_eval");
                let cond_block = self.context.append_basic_block(function, "for_cond");
                let body_block = self.context.append_basic_block(function, "for_body");
                let end_block = self.context.append_basic_block(function, "for_end");

                // Generate expression and allocate in entry block BEFORE branching
                // to ensure alloca is in entry block (not in loop)
                let iter_val = self.generate_hir_expr(iterable)?;
                let iter_type = iter_val.get_type();

                // Allocate variable to store iteration state or option value
                // IMPORTANT: Allocate in entry block, NOT in for_eval block
                // to avoid stack overflow in infinite loops where for_eval is visited repeatedly
                let iter_alloca = self.builder.build_alloca(iter_type, "iter_var")?;
                // Store the initial value
                self.builder.build_store(iter_alloca, iter_val)?;

                // Branch to for_eval (now in entry block, after alloca)
                self.builder.build_unconditional_branch(eval_start_block)?;

                // Now position at for_eval for rest of loop setup
                self.builder.position_at_end(eval_start_block);

                // Get the Lang type from the HIR expression to check if it's an array
                let iter_lang_type = match iterable {
                    hir::HirExpr::Int(_, ty, _) => Some(ty),
                    hir::HirExpr::Float(_, ty, _) => Some(ty),
                    hir::HirExpr::Bool(_, ty, _) => Some(ty),
                    hir::HirExpr::String(_, ty, _) => Some(ty),
                    hir::HirExpr::Char(_, ty, _) => Some(ty),
                    hir::HirExpr::Null(ty, _) => Some(ty),
                    hir::HirExpr::Ident(_, ty, _) => Some(ty),
                    hir::HirExpr::Tuple { ty, .. } => Some(ty),
                    hir::HirExpr::TupleIndex { ty, .. } => Some(ty),
                    hir::HirExpr::Array { ty, .. } => Some(ty),
                    hir::HirExpr::Binary { ty, .. } => Some(ty),
                    hir::HirExpr::Unary { ty, .. } => Some(ty),
                    hir::HirExpr::Call { return_ty, .. } => Some(return_ty),
                    hir::HirExpr::If { ty, .. } => Some(ty),
                    hir::HirExpr::Block { ty, .. } => Some(ty),
                    hir::HirExpr::MemberAccess { ty, .. } => Some(ty),
                    hir::HirExpr::Struct { ty, .. } => Some(ty),
                    hir::HirExpr::Try { .. } => None,
                    hir::HirExpr::Catch { .. } => None,
                };

                // Analyze type
                let mut is_option = false;
                let mut is_bool = false;
                let mut is_array = false;

                // Check using Lang type first
                if let Some(lang_ty) = iter_lang_type {
                    if let Type::Array { .. } = lang_ty {
                        is_array = true;
                    }
                }

                // Also check LLVM type
                if !is_array {
                    if iter_type.is_struct_type() {
                        let struct_type = iter_type.into_struct_type();
                        let types = struct_type.get_field_types();
                        if types.len() == 2 {
                            if let Some(t) = types.get(1) {
                                if t.is_int_type() && t.into_int_type().get_bit_width() == 1 {
                                    is_option = true;
                                }
                            }
                        }
                    } else if iter_type.is_int_type()
                        && iter_type.into_int_type().get_bit_width() == 1
                    {
                        is_bool = true;
                    } else if iter_type.is_array_type() {
                        is_array = true;
                    } else if iter_type.is_pointer_type() {
                        // Arrays can be passed as pointers - try to handle this case too
                        // For now, treat as unsupported/infinite loop
                    }
                }

                // For arrays, we need an index variable
                let array_index_alloca = if is_array {
                    let idx = self
                        .builder
                        .build_alloca(self.context.i64_type(), "array_index")?;
                    self.builder
                        .build_store(idx, self.context.i64_type().const_int(0, false))?;
                    Some(idx)
                } else {
                    None
                };
                self.builder.build_unconditional_branch(cond_block)?;

                // Now setup the loop structure tracking for breaks
                let label_clone = label.clone();
                self.loop_end_blocks.push(vec![(end_block, label_clone)]);

                // Condition block
                self.builder.position_at_end(cond_block);
                let iter_val_load = self
                    .builder
                    .build_load(iter_type, iter_alloca, "iter_load")?;

                if is_option {
                    // This is an Option type - check if it's null (is_valid = false)
                    let is_valid = self
                        .builder
                        .build_extract_value(iter_val_load.into_struct_value(), 1, "is_valid")?
                        .into_int_value();
                    let is_null = self.builder.build_int_compare(
                        inkwell::IntPredicate::EQ,
                        is_valid,
                        self.context.bool_type().const_int(0, false),
                        "is_null",
                    )?;
                    // If null (is_valid = false), it's an infinite loop - branch to body
                    // If not null (is_valid = true), exit the loop - branch to end
                    self.builder
                        .build_conditional_branch(is_null, body_block, end_block)?;
                } else if is_bool {
                    self.builder.build_conditional_branch(
                        iter_val_load.into_int_value(),
                        body_block,
                        end_block,
                    )?;
                } else if iter_type.is_struct_type() {
                    // Range, Option, or Null (infinite loop)
                    let struct_type = iter_type.into_struct_type();
                    let types = struct_type.get_field_types();
                    let mut handled_null_or_option = false;
                    if types.len() == 2 {
                        if let Some(t) = types.get(1) {
                            if t.is_int_type() && t.into_int_type().get_bit_width() == 1 {
                                // This is an Option type - check if it's null (is_valid = false)
                                let is_valid = self.builder.build_extract_value(
                                    iter_val_load.into_struct_value(),
                                    1,
                                    "is_valid",
                                )?;
                                let is_valid_flag = is_valid.into_int_value();
                                let is_null = self.builder.build_int_compare(
                                    inkwell::IntPredicate::EQ,
                                    is_valid_flag,
                                    self.context.bool_type().const_int(0, false),
                                    "is_null",
                                )?;
                                // If null (is_valid = false), it's an infinite loop - branch to body
                                // If not null (is_valid = true), exit the loop - branch to end
                                self.builder
                                    .build_conditional_branch(is_null, end_block, body_block)?;
                                handled_null_or_option = true;
                            }
                        }
                    }
                    // Only handle as Range if we didn't handle as Option/Null
                    if !handled_null_or_option {
                        // Range
                        let start = self.builder.build_extract_value(
                            iter_val_load.into_struct_value(),
                            0,
                            "range_start",
                        )?;
                        let end = self.builder.build_extract_value(
                            iter_val_load.into_struct_value(),
                            1,
                            "range_end",
                        )?;
                        let condition_flag = self.builder.build_int_compare(
                            inkwell::IntPredicate::SLT,
                            start.into_int_value(),
                            end.into_int_value(),
                            "range_cmp",
                        )?;
                        self.builder.build_conditional_branch(
                            condition_flag,
                            body_block,
                            end_block,
                        )?;
                    }
                } else if is_array || iter_type.is_array_type() {
                    // Array iteration: check index < array length
                    if let Some(idx_alloca) = array_index_alloca {
                        let current_index = self.builder.build_load(
                            self.context.i64_type(),
                            idx_alloca,
                            "array_idx_load",
                        )?;
                        // Get array length
                        let array_type = iter_type.into_array_type();
                        let len = array_type.len();
                        let len_val = self.context.i64_type().const_int(len as u64, false);
                        let condition_flag = self.builder.build_int_compare(
                            inkwell::IntPredicate::SLT,
                            current_index.into_int_value(),
                            len_val,
                            "array_cmp",
                        )?;
                        self.builder.build_conditional_branch(
                            condition_flag,
                            body_block,
                            end_block,
                        )?;
                    } else {
                        self.builder.build_unconditional_branch(body_block)?;
                    }
                } else if iter_type.is_pointer_type() {
                    // Pointer type (e.g., null/Pointer<Void>) represents an infinite loop
                    // Always branch to the body block
                    self.builder.build_unconditional_branch(body_block)?;
                } else {
                    // Fallback for unsupported types - exit the loop to prevent infinite loop
                    self.builder.build_unconditional_branch(end_block)?;
                }

                // Body block
                self.builder.position_at_end(body_block);

                // Setup loop variables (capture, index)
                let var_name_clone = var_name.clone();
                if let Some(name) = &var_name_clone {
                    if is_option || iter_type.is_struct_type() {
                        let val = self.builder.build_extract_value(
                            iter_val_load.into_struct_value(),
                            0,
                            "captured",
                        )?;
                        let alloca = self.builder.build_alloca(val.get_type(), name)?;
                        self.builder.build_store(alloca, val)?;
                        self.variables.insert(name.clone(), alloca);
                        self.variable_types.insert(name.clone(), Type::I64); // Fallback type
                    } else if is_array {
                        // Array: extract element at current index
                        if let Some(idx_alloca) = array_index_alloca {
                            let current_index = self.builder.build_load(
                                self.context.i64_type(),
                                idx_alloca,
                                "array_idx_load_var",
                            )?;
                            // Use GEP to get element pointer
                            let array_type = iter_type.into_array_type();
                            let element_type = array_type.get_element_type();
                            let ptr = unsafe {
                                self.builder.build_in_bounds_gep(
                                    iter_type,
                                    iter_alloca,
                                    &[
                                        self.context.i64_type().const_int(0, false),
                                        current_index.into_int_value(),
                                    ],
                                    "array_elem_ptr",
                                )?
                            };
                            let elem_val =
                                self.builder.build_load(element_type, ptr, "array_elem")?;
                            let alloca = self.builder.build_alloca(element_type, name)?;
                            self.builder.build_store(alloca, elem_val)?;
                            self.variables.insert(name.clone(), alloca);
                            // Get the proper element type for the variable type
                            let element_lang_type = match element_type {
                                BasicTypeEnum::IntType(t) => match t.get_bit_width() {
                                    8 => Type::I8,
                                    16 => Type::I16,
                                    32 => Type::I32,
                                    64 => Type::I64,
                                    _ => Type::I64,
                                },
                                BasicTypeEnum::FloatType(_) => Type::F64,
                                _ => Type::I64,
                            };
                            self.variable_types.insert(name.clone(), element_lang_type);
                        } else {
                            // Dummy
                            let alloca =
                                self.builder.build_alloca(self.context.i64_type(), name)?;
                            self.builder
                                .build_store(alloca, self.context.i64_type().const_zero())?;
                            self.variables.insert(name.clone(), alloca);
                            self.variable_types.insert(name.clone(), Type::I64);
                        }
                    } else {
                        // Dummy
                        let alloca = self.builder.build_alloca(self.context.i64_type(), name)?;
                        self.builder
                            .build_store(alloca, self.context.i64_type().const_zero())?;
                        self.variables.insert(name.clone(), alloca);
                        self.variable_types.insert(name.clone(), Type::I64);
                    }
                }

                let index_var_clone = index_var.clone();
                if let Some(name) = &index_var_clone {
                    if iter_type.is_struct_type() {
                        let val = self.builder.build_extract_value(
                            iter_val_load.into_struct_value(),
                            1,
                            "index_captured",
                        )?;
                        let alloca = self.builder.build_alloca(val.get_type(), name)?;
                        self.builder.build_store(alloca, val)?;
                        self.variables.insert(name.clone(), alloca);
                        self.variable_types.insert(name.clone(), Type::I64); // Fallback type
                    } else if is_array {
                        // Array: use the index variable
                        if let Some(idx_alloca) = array_index_alloca {
                            // Load the current index
                            let current_index = self.builder.build_load(
                                self.context.i64_type(),
                                idx_alloca,
                                "array_idx_for_var",
                            )?;
                            let alloca =
                                self.builder.build_alloca(self.context.i64_type(), name)?;
                            self.builder.build_store(alloca, current_index)?;
                            self.variables.insert(name.clone(), alloca);
                            self.variable_types.insert(name.clone(), Type::I64);
                        } else {
                            // Dummy
                            let alloca =
                                self.builder.build_alloca(self.context.i64_type(), name)?;
                            self.builder
                                .build_store(alloca, self.context.i64_type().const_zero())?;
                            self.variables.insert(name.clone(), alloca);
                            self.variable_types.insert(name.clone(), Type::I64);
                        }
                    } else {
                        // Dummy
                        let alloca = self.builder.build_alloca(self.context.i64_type(), name)?;
                        self.builder
                            .build_store(alloca, self.context.i64_type().const_zero())?;
                        self.variables.insert(name.clone(), alloca);
                        self.variable_types.insert(name.clone(), Type::I64);
                    }
                }

                self.generate_hir_stmt(body)?;

                let current_block = self.builder.get_insert_block().unwrap();
                if current_block.get_terminator().is_none() {
                    if is_option || is_bool {
                        // Option or Bool: jump back to EVALUATION block to evaluate again!
                        self.builder.build_unconditional_branch(eval_start_block)?;
                    } else if iter_type.is_struct_type() {
                        // Range: increment the counter
                        let current_load =
                            self.builder
                                .build_load(iter_type, iter_alloca, "iter_next")?;
                        let current_struct = current_load.into_struct_value();
                        let start =
                            self.builder
                                .build_extract_value(current_struct, 0, "next_start")?;
                        let end =
                            self.builder
                                .build_extract_value(current_struct, 1, "next_end")?;
                        let incremented_start = self.builder.build_int_add(
                            start.into_int_value(),
                            self.context.i64_type().const_int(1, false),
                            "start_inc",
                        )?;

                        let mut new_struct: inkwell::values::AggregateValueEnum =
                            iter_type.into_struct_type().const_zero().into();
                        new_struct = self
                            .builder
                            .build_insert_value(
                                new_struct.into_struct_value(),
                                incremented_start,
                                0,
                                "new_iter",
                            )?
                            .into();
                        new_struct = self
                            .builder
                            .build_insert_value(
                                new_struct.into_struct_value(),
                                end,
                                1,
                                "new_iter_final",
                            )?
                            .into();
                        self.builder
                            .build_store(iter_alloca, new_struct.as_basic_value_enum())?;

                        self.builder.build_unconditional_branch(cond_block)?;
                    } else if is_array {
                        // Array: increment the index
                        if let Some(idx_alloca) = array_index_alloca {
                            let current_index = self.builder.build_load(
                                self.context.i64_type(),
                                idx_alloca,
                                "array_idx_inc",
                            )?;
                            let incremented = self.builder.build_int_add(
                                current_index.into_int_value(),
                                self.context.i64_type().const_int(1, false),
                                "array_idx_next",
                            )?;
                            self.builder.build_store(idx_alloca, incremented)?;
                        }
                        self.builder.build_unconditional_branch(cond_block)?;
                    } else {
                        // Unsupported
                        self.builder.build_unconditional_branch(cond_block)?;
                    }
                }

                // Merge
                self.builder.position_at_end(end_block);
                self.loop_end_blocks.pop();

                // Clean up variables
                if let Some(name) = &var_name_clone {
                    self.variables.remove(name);
                    self.variable_types.remove(name);
                }
                if let Some(name) = &index_var_clone {
                    self.variables.remove(name);
                    self.variable_types.remove(name);
                }

                Ok(())
            }
            hir::HirStmt::Switch {
                condition, cases, ..
            } => {
                let function = self.current_function.unwrap();
                let end_block = self.context.append_basic_block(function, "switch_end");
                let cond_val = self.generate_hir_expr(condition)?;

                for case in cases {
                    let body_block = self.context.append_basic_block(function, "case_body");
                    let next_case_block = self.context.append_basic_block(function, "next_case");

                    // Check if wildcard case
                    let mut is_wildcard = false;
                    for pattern in &case.patterns {
                        if let hir::HirExpr::Ident(name, ..) = pattern {
                            if name == "_" {
                                is_wildcard = true;
                                break;
                            }
                        }
                    }

                    if is_wildcard {
                        self.builder.build_unconditional_branch(body_block)?;
                    } else {
                        // Create comparison for each pattern in the case (OR-ed together)
                        let mut combined_cond: Option<inkwell::values::IntValue> = None;
                        for pattern in &case.patterns {
                            let pattern_val = self.generate_hir_expr(pattern)?;

                            // Check if pattern is a Range (BinaryOp::Range)
                            let cmp = if let hir::HirExpr::Binary {
                                op: crate::ast::BinaryOp::Range,
                                left,
                                right,
                                ..
                            } = pattern
                            {
                                // Range pattern: check if value is in range [start, end)
                                let start = self.generate_hir_expr(left)?;
                                let end = self.generate_hir_expr(right)?;
                                let start_int = start.into_int_value();
                                let end_int = end.into_int_value();
                                let cond_int = cond_val.into_int_value();

                                // Check: start <= cond < end
                                let ge = self.builder.build_int_compare(
                                    inkwell::IntPredicate::UGE,
                                    cond_int,
                                    start_int,
                                    "range_ge",
                                )?;
                                let lt = self.builder.build_int_compare(
                                    inkwell::IntPredicate::ULT,
                                    cond_int,
                                    end_int,
                                    "range_lt",
                                )?;
                                self.builder.build_and(ge, lt, "range_check")?
                            } else {
                                // Regular equality comparison
                                self.builder.build_int_compare(
                                    inkwell::IntPredicate::EQ,
                                    cond_val.into_int_value(),
                                    pattern_val.into_int_value(),
                                    "case_cmp",
                                )?
                            };
                            combined_cond = if let Some(c) = combined_cond {
                                Some(self.builder.build_or(c, cmp, "or")?)
                            } else {
                                Some(cmp)
                            };
                        }

                        if let Some(cond) = combined_cond {
                            self.builder.build_conditional_branch(
                                cond,
                                body_block,
                                next_case_block,
                            )?;
                        } else {
                            self.builder.build_unconditional_branch(next_case_block)?;
                        }
                    }

                    // Case body
                    self.builder.position_at_end(body_block);
                    self.generate_hir_stmt(&case.body)?;
                    if self
                        .builder
                        .get_insert_block()
                        .unwrap()
                        .get_terminator()
                        .is_none()
                    {
                        self.builder.build_unconditional_branch(end_block)?;
                    }

                    // Position builder for the next case check
                    self.builder.position_at_end(next_case_block);
                }

                // Final fallthrough (if no case matches)
                if self
                    .builder
                    .get_insert_block()
                    .unwrap()
                    .get_terminator()
                    .is_none()
                {
                    self.builder.build_unconditional_branch(end_block)?;
                }

                // End block
                self.builder.position_at_end(end_block);
                Ok(())
            }
            hir::HirStmt::Defer { stmt, .. } => {
                // Add the deferred statement to the current scope's defer list
                // It will be executed in LIFO order when the scope exits
                self.add_defer((**stmt).clone());
                Ok(())
            }
            hir::HirStmt::DeferBang { stmt, .. } => {
                // Add the deferred! statement to the current scope's defer! list
                // It will be executed only when an error occurs (try returns error but not caught)
                self.add_defer_bang((**stmt).clone());
                Ok(())
            }
            hir::HirStmt::Break { label, span } => {
                // For labeled breaks, search through all loop levels
                // For unlabeled breaks, use the innermost loop
                let mut target_block = None;

                // Iterate through all loop levels (from innermost to outermost)
                for loop_stack in self.loop_end_blocks.iter().rev() {
                    // Find the appropriate end block in this loop level
                    if let Some(block) = loop_stack.iter().find_map(|(block, l)| {
                        if l.as_ref() == label.as_ref() {
                            Some(*block)
                        } else if label.is_none() {
                            // If no label specified, use this loop
                            Some(*block)
                        } else {
                            None
                        }
                    }) {
                        target_block = Some(block);
                        break;
                    }
                }

                if let Some(target_block) = target_block {
                    self.builder.build_unconditional_branch(target_block)?;
                } else if label.is_some() {
                    return Err(format!(
                        "break statement with label '{}' not found in scope at span {:?}",
                        label.as_deref().unwrap(),
                        span
                    )
                    .into());
                } else {
                    return Err(
                        format!("break statement outside of loop at span {:?}", span).into(),
                    );
                }
                Ok(())
            }
        }
    }

    fn generate_hir_expr(&mut self, expr: &hir::HirExpr) -> CodegenResult<BasicValueEnum<'ctx>> {
        match expr {
            hir::HirExpr::Int(v, ty, _) => Ok(self.build_typed_int_constant(*v, ty)),
            hir::HirExpr::Float(v, _, _) => Ok(self.context.f64_type().const_float(*v).into()),
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
            hir::HirExpr::Char(v, ty, _) => {
                // Use the type from the HIR expression if available
                match self.llvm_type(ty) {
                    BasicTypeEnum::IntType(_) => Ok(self.build_typed_int_constant(*v as i64, ty)),
                    _ => Ok(self.context.i64_type().const_int(*v as u64, false).into()),
                }
            }
            hir::HirExpr::Null(ty, _) => {
                // Null is represented as a struct { value, is_valid } with is_valid = false
                // Use the expected type from the Option if context available
                let (val_type, is_valid_type) = if let Type::Option(inner) = ty {
                    (self.llvm_type(inner), self.context.bool_type())
                } else {
                    (self.context.i64_type().into(), self.context.bool_type())
                };
                let null_struct = self
                    .context
                    .struct_type(&[val_type.into(), is_valid_type.into()], false);
                Ok(null_struct.const_zero().into())
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
                let tuple_val = self.generate_hir_expr(tuple)?;
                let _llvm_type = self.llvm_type(ty);
                match tuple_val {
                    BasicValueEnum::StructValue(sv) => {
                        let extracted =
                            self.builder
                                .build_extract_value(sv, *index as u32, "tuple_elem")?;
                        Ok(extracted.into())
                    }
                    _ => Err("Tuple index access requires a tuple value".into()),
                }
            }
            hir::HirExpr::Array { vals, ty, .. } => {
                // Create an LLVM array from values
                let llvm_type = self.llvm_type(ty);
                let expected_element_type = match ty {
                    Type::Array { element_type, .. } => element_type.as_ref(),
                    _ => return Err("Array expression is missing an array type".into()),
                };

                // Generate element values
                let mut elem_vals: Vec<inkwell::values::BasicValueEnum> = Vec::new();
                for v in vals {
                    let raw_val = self.generate_hir_expr(v)?;
                    elem_vals.push(self.coerce_type(raw_val, expected_element_type)?);
                }

                // Create array constant using const_array
                let array_type = llvm_type.into_array_type();
                let element_type = array_type.get_element_type();
                let array_val = match element_type {
                    BasicTypeEnum::IntType(t) => {
                        let mut const_vec: Vec<inkwell::values::IntValue> = Vec::new();
                        for ev in &elem_vals {
                            const_vec.push(ev.into_int_value());
                        }
                        t.const_array(&const_vec).as_basic_value_enum()
                    }
                    BasicTypeEnum::FloatType(t) => {
                        let mut const_vec: Vec<inkwell::values::FloatValue> = Vec::new();
                        for ev in &elem_vals {
                            const_vec.push(ev.into_float_value());
                        }
                        t.const_array(&const_vec).as_basic_value_enum()
                    }
                    _ => return Err("Unsupported array element type".into()),
                };

                // Store constant to memory and return loaded value
                let alloca = self.builder.build_alloca(llvm_type, "array")?;
                self.builder.build_store(alloca, array_val)?;
                Ok(self
                    .builder
                    .build_load(llvm_type, alloca, "array_load")?
                    .into())
            }
            hir::HirExpr::Ident(name, ty, _) => {
                // Skip underscore identifier (used for ignoring values)
                if name == "_" {
                    // Return a dummy value
                    return Ok(self.context.i64_type().const_zero().into());
                }

                // Check if this is a function type - if so, get the function pointer
                if let Type::Function { .. } = ty {
                    // Get the function pointer
                    let fn_name = if name == "main" {
                        "main".to_string()
                    } else {
                        format!("{}_{}", self.module_name, name)
                    };
                    let fn_val = self
                        .module
                        .get_function(&fn_name)
                        .ok_or(format!("Function not found: {}", fn_name))?;
                    // Return the function's pointer
                    return Ok(fn_val.as_global_value().as_pointer_value().into());
                }

                let ptr = self
                    .variables
                    .get(name)
                    .ok_or(format!("Var not found: {}", name))?;
                let var_ty = self.variable_types.get(name).unwrap();
                let llvm_type = self.llvm_type(var_ty);
                Ok(self.builder.build_load(llvm_type, *ptr, name)?.into())
            }
            hir::HirExpr::Binary {
                op,
                left,
                right,
                ty,
                ..
            } => {
                let mut l = self.generate_hir_expr(left)?;
                let mut r = self.generate_hir_expr(right)?;

                // Type coercion: cast right operand to match left operand's type if needed
                if l.is_int_value() && r.is_int_value() {
                    let l_type = l.into_int_value().get_type();
                    let r_type = r.into_int_value().get_type();
                    if l_type.get_bit_width() != r_type.get_bit_width() {
                        r = self
                            .builder
                            .build_int_cast(r.into_int_value(), l_type, "binary_cast")?
                            .into();
                    }
                }

                // Handle different types
                let val = match op {
                    BinaryOp::Add => {
                        if l.is_float_value() {
                            self.builder
                                .build_float_add(
                                    l.into_float_value(),
                                    r.into_float_value(),
                                    "fadd",
                                )?
                                .into()
                        } else {
                            let l_int = l.into_int_value();
                            let r_int = r.into_int_value();
                            self.builder.build_int_add(l_int, r_int, "add")?.into()
                        }
                    }
                    BinaryOp::Sub => {
                        if l.is_float_value() {
                            self.builder
                                .build_float_sub(
                                    l.into_float_value(),
                                    r.into_float_value(),
                                    "fsub",
                                )?
                                .into()
                        } else {
                            let l_int = l.into_int_value();
                            let r_int = r.into_int_value();
                            self.builder.build_int_sub(l_int, r_int, "sub")?.into()
                        }
                    }
                    BinaryOp::Mul => {
                        if l.is_float_value() {
                            self.builder
                                .build_float_mul(
                                    l.into_float_value(),
                                    r.into_float_value(),
                                    "fmul",
                                )?
                                .into()
                        } else {
                            let l_int = l.into_int_value();
                            let r_int = r.into_int_value();
                            self.builder.build_int_mul(l_int, r_int, "mul")?.into()
                        }
                    }
                    BinaryOp::Div => {
                        if l.is_float_value() {
                            self.builder
                                .build_float_div(
                                    l.into_float_value(),
                                    r.into_float_value(),
                                    "fdiv",
                                )?
                                .into()
                        } else {
                            let l_int = l.into_int_value();
                            let r_int = r.into_int_value();
                            self.builder
                                .build_int_unsigned_div(l_int, r_int, "div")?
                                .into()
                        }
                    }
                    BinaryOp::Mod => {
                        if l.is_float_value() {
                            self.builder
                                .build_float_rem(
                                    l.into_float_value(),
                                    r.into_float_value(),
                                    "frem",
                                )?
                                .into()
                        } else {
                            let l_int = l.into_int_value();
                            let r_int = r.into_int_value();
                            self.builder
                                .build_int_unsigned_rem(l_int, r_int, "mod")?
                                .into()
                        }
                    }
                    BinaryOp::Eq => {
                        if l.is_float_value() {
                            self.builder
                                .build_float_compare(
                                    inkwell::FloatPredicate::OEQ,
                                    l.into_float_value(),
                                    r.into_float_value(),
                                    "feq",
                                )?
                                .into()
                        } else {
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
                    }
                    BinaryOp::Ne => {
                        if l.is_float_value() {
                            self.builder
                                .build_float_compare(
                                    inkwell::FloatPredicate::ONE,
                                    l.into_float_value(),
                                    r.into_float_value(),
                                    "fne",
                                )?
                                .into()
                        } else {
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
                    }
                    BinaryOp::Lt => {
                        if l.is_float_value() {
                            self.builder
                                .build_float_compare(
                                    inkwell::FloatPredicate::OLT,
                                    l.into_float_value(),
                                    r.into_float_value(),
                                    "flt",
                                )?
                                .into()
                        } else {
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
                    }
                    BinaryOp::Gt => {
                        if l.is_float_value() {
                            self.builder
                                .build_float_compare(
                                    inkwell::FloatPredicate::OGT,
                                    l.into_float_value(),
                                    r.into_float_value(),
                                    "fgt",
                                )?
                                .into()
                        } else {
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
                    }
                    BinaryOp::Le => {
                        if l.is_float_value() {
                            self.builder
                                .build_float_compare(
                                    inkwell::FloatPredicate::OLE,
                                    l.into_float_value(),
                                    r.into_float_value(),
                                    "fle",
                                )?
                                .into()
                        } else {
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
                    }
                    BinaryOp::Ge => {
                        if l.is_float_value() {
                            self.builder
                                .build_float_compare(
                                    inkwell::FloatPredicate::OGE,
                                    l.into_float_value(),
                                    r.into_float_value(),
                                    "fge",
                                )?
                                .into()
                        } else {
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
                    BinaryOp::BitAnd => {
                        let l_int = l.into_int_value();
                        let r_int = r.into_int_value();
                        self.builder.build_and(l_int, r_int, "bitand")?.into()
                    }
                    BinaryOp::BitOr => {
                        let l_int = l.into_int_value();
                        let r_int = r.into_int_value();
                        self.builder.build_or(l_int, r_int, "bitor")?.into()
                    }
                    BinaryOp::BitXor => {
                        let l_int = l.into_int_value();
                        let r_int = r.into_int_value();
                        self.builder.build_xor(l_int, r_int, "bitxor")?.into()
                    }
                    BinaryOp::Shl => {
                        let l_int = l.into_int_value();
                        let r_int = r.into_int_value();
                        self.builder.build_left_shift(l_int, r_int, "shl")?.into()
                    }
                    BinaryOp::Shr => {
                        let l_int = l.into_int_value();
                        let r_int = r.into_int_value();
                        // Use logical shift right (LSR) for now, as we use unsigned divisions
                        self.builder
                            .build_right_shift(l_int, r_int, false, "shr")?
                            .into()
                    }
                    BinaryOp::Range => {
                        // Range: create a tuple { start, end }
                        let l_val = l.into_int_value();
                        let r_val = r.into_int_value();
                        // Create a tuple struct type { i64, i64 }
                        let tuple_type = self.context.struct_type(
                            &[
                                self.context.i64_type().into(),
                                self.context.i64_type().into(),
                            ],
                            false,
                        );
                        // Create the struct value
                        let tuple_val = self
                            .context
                            .const_struct(&[l_val.into(), r_val.into()], false);
                        // Store to memory and return
                        let alloca = self.builder.build_alloca(tuple_type, "range")?;
                        self.builder.build_store(alloca, tuple_val)?;
                        self.builder
                            .build_load(tuple_type, alloca, "range_load")?
                            .into()
                    }
                };
                Ok(val)
            }
            hir::HirExpr::Unary {
                op, expr, ty: _, ..
            } => {
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
                // First, check if the callee is a variable that holds a function pointer
                if namespace.is_none() {
                    if let Some(var_alloca) = self.variables.get(name) {
                        if let Some(var_type) = self.variable_types.get(name) {
                            // Check if this variable has a function type
                            if let Type::Function {
                                params,
                                return_type,
                            } = var_type.clone()
                            {
                                // Load the function pointer from the variable
                                let fn_ptr = self.builder.build_load(
                                    self.context.ptr_type(inkwell::AddressSpace::default()),
                                    *var_alloca,
                                    name,
                                )?;

                                // Generate args first
                                let mut llvm_args: Vec<BasicMetadataValueEnum> = Vec::new();
                                for arg_expr in args {
                                    let val = self.generate_hir_expr(arg_expr)?;
                                    llvm_args.push(BasicMetadataValueEnum::from(val));
                                }

                                // Create function type for indirect call using existing method
                                let fn_sig = self.build_function_type(&return_type, &params, false);

                                // Cast pointer to function pointer type
                                let casted_ptr = self
                                    .builder
                                    .build_bit_cast(
                                        fn_ptr,
                                        self.context.ptr_type(inkwell::AddressSpace::default()),
                                        "cast_fn",
                                    )?
                                    .into_pointer_value();

                                let call_result = self.builder.build_indirect_call(
                                    fn_sig,
                                    casted_ptr,
                                    &llvm_args,
                                    "indirect_call",
                                )?;

                                let result = match call_result.try_as_basic_value() {
                                    inkwell::values::ValueKind::Basic(val) => val,
                                    _ => self.context.i64_type().const_int(0, false).into(),
                                };
                                return Ok(result);
                            }
                        }
                    }
                }

                let (mangled_name, _is_std, needs_self, is_fn_ptr_field) =
                    if let Some(ns) = namespace.as_deref() {
                        // First, check if the namespace is a variable with a struct type
                        // This handles method calls like "f.next()" where f is a struct instance
                        // But also handles field access like "c.add" where c is a struct with a function field
                        if let Some(var_type) = self.variable_types.get(ns) {
                            if let Type::Custom {
                                name: type_name, ..
                            } = var_type
                            {
                                // Check if this is a known method - try to find it
                                let method_name = format!("{}_{}", type_name, name);
                                let mangled = self.mangle_name(&method_name, false);

                                // Check if the method actually exists in the module
                                let method_exists = self.module.get_function(&mangled).is_some();

                                if method_exists {
                                    (mangled, false, true, false)
                                } else {
                                    // Method doesn't exist - this is likely a function pointer field
                                    // Return a marker to handle it as field access
                                    (format!("{}__fn_ptr_field", name), false, false, true)
                                }
                            } else {
                                // Not a custom type, use the namespace as-is
                                let actual_package = self
                                    .imported_packages
                                    .get(ns)
                                    .map(|s| s.as_str())
                                    .unwrap_or(ns);

                                if actual_package == "io" && name == "println" {
                                    return self.generate_hir_io_println(args);
                                }

                                (format!("{}_{}", actual_package, name), true, false, false)
                            }
                        } else if let Some(actual_package) = self.imported_packages.get(ns) {
                            // Namespace is an imported package
                            if actual_package == "io" && name == "println" {
                                return self.generate_hir_io_println(args);
                            }
                            (format!("{}_{}", actual_package, name), true, false, false)
                        } else {
                            // Namespace is not a known variable or package - likely a local struct/enum or implicit built-in
                            if ns == "io" && name == "println" {
                                return self.generate_hir_io_println(args);
                            }
                            let combined_name = format!("{}_{}", ns, name);
                            (self.mangle_name(&combined_name, false), false, false, false)
                        }
                    } else {
                        if name == "main" {
                            ("main".to_string(), false, false, false)
                        } else if name == "is_null" || name == "is_not_null" {
                            // Built-in null checking functions - handled inline
                            return self.generate_null_check_builtin(name, args);
                        } else {
                            (
                                format!("{}_{}", self.module_name, name),
                                false,
                                false,
                                false,
                            )
                        }
                    };

                // Handle function pointer field access
                if is_fn_ptr_field {
                    if let Some(ns) = namespace.as_deref() {
                        // Get the struct variable
                        let struct_ptr = self
                            .variables
                            .get(ns)
                            .or_else(|| self.const_variables.get(ns))
                            .copied()
                            .ok_or_else(|| format!("Variable not found: {}", ns))?;

                        let var_ty = self
                            .variable_types
                            .get(ns)
                            .ok_or_else(|| format!("Type not found for: {}", ns))?;

                        // Get the field index from the struct type
                        if let Type::Custom {
                            name: type_name, ..
                        } = var_ty
                        {
                            // Look up the struct type
                            if let Some(struct_type) = self.context.get_struct_type(type_name) {
                                // Find the field index - we need to iterate through struct fields
                                // For now, assume field 0 (add field)
                                // TODO: Make this more robust by finding the actual field
                                let field_idx = 0u32; // Assuming 'add' is the first field

                                // GEP to the field
                                let field_ptr = self
                                    .builder
                                    .build_struct_gep(
                                        struct_type,
                                        struct_ptr,
                                        field_idx,
                                        "field_ptr",
                                    )
                                    .map_err(|e| e.to_string())?;

                                // Load the function pointer
                                let fn_ptr_type =
                                    self.context.ptr_type(inkwell::AddressSpace::default());
                                let fn_ptr = self
                                    .builder
                                    .build_load(fn_ptr_type, field_ptr, "fn_ptr")?
                                    .into_pointer_value();

                                // Get the function type from HIR
                                // For now, use a simple i64 function type
                                // TODO: Get the actual function signature
                                let fn_sig = self.context.i64_type().fn_type(&[], false);

                                // Generate args first
                                let mut llvm_args: Vec<BasicMetadataValueEnum> = Vec::new();
                                for arg_expr in args {
                                    let val = self.generate_hir_expr(arg_expr)?;
                                    llvm_args.push(BasicMetadataValueEnum::from(val));
                                }

                                // Make indirect call
                                let call_result = self.builder.build_indirect_call(
                                    fn_sig,
                                    fn_ptr,
                                    &llvm_args,
                                    "indirect_call",
                                )?;

                                let result = match call_result.try_as_basic_value() {
                                    inkwell::values::ValueKind::Basic(val) => val,
                                    _ => self.context.i64_type().const_int(0, false).into(),
                                };
                                return Ok(result);
                            }
                        }
                    }
                }

                let function = self
                    .module
                    .get_function(&mangled_name)
                    .or_else(|| self.module.get_function(name)) // fallback demangled
                    .ok_or(format!(
                        "Fn not found: {} (original: {})",
                        mangled_name, name
                    ))?;

                let mut llvm_args = Vec::new();

                // If this is a method call on a struct instance, add self as first argument
                if needs_self {
                    if let Some(ns) = namespace.as_deref() {
                        if let Some(ptr) = self.variables.get(ns) {
                            // For methods, self is usually a pointer to the struct
                            // ONLY add self if the function actually expects it
                            if function.count_params() == args.len() as u32 + 1 {
                                llvm_args.push(BasicMetadataValueEnum::from(*ptr));
                            }
                        }
                    }
                }

                for arg in args {
                    let val = self.generate_hir_expr(arg)?;
                    llvm_args.push(BasicMetadataValueEnum::from(val));
                }

                let call_result = self.builder.build_call(function, &llvm_args, "call")?;
                let result = match call_result.try_as_basic_value() {
                    inkwell::values::ValueKind::Basic(val) => val,
                    _ => self.context.i64_type().const_int(0, false).into(),
                };
                Ok(result)
            }
            hir::HirExpr::If {
                condition,
                capture,
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

                // Convert condition to i1 for branching
                let is_true = self.condition_to_i1(cond_val, "is_true")?;
                self.builder
                    .build_conditional_branch(is_true, then_block, else_block)?;

                // Then branch
                self.builder.position_at_end(then_block);

                // Handle capture variable if present (e.g., if (opt) |data| { ... })
                let mut old_var = None;
                let mut old_ty = None;
                let capture_name = capture.clone();
                if let Some(ref name) = capture_name {
                    if let BasicValueEnum::StructValue(sv) = cond_val {
                        // Extract the value (first element) from the struct
                        let val = self.builder.build_extract_value(sv, 0, "captured")?;
                        let alloca = self.builder.build_alloca(val.get_type(), name)?;
                        self.builder.build_store(alloca, val)?;
                        // Save old variable if it exists
                        old_var = self.variables.insert(name.clone(), alloca);
                        old_ty = self
                            .variable_types
                            .insert(name.clone(), self.llvm_type_to_lang(&val.get_type()));
                    }
                }

                let result_type = self.llvm_type(ty);

                let then_val = self.generate_hir_expr(then_branch)?;
                let then_val_coerced = self.coerce_to_llvm_type(then_val, result_type)?;
                let then_actual_block = self.builder.get_insert_block().unwrap();
                self.builder.build_unconditional_branch(merge_block)?;

                // Restore variable if it was shadowed
                if let Some(ref name) = capture_name {
                    if let Some(old) = old_var {
                        self.variables.insert(name.clone(), old);
                    } else {
                        self.variables.remove(name);
                    }

                    if let Some(old) = old_ty {
                        self.variable_types.insert(name.clone(), old);
                    } else {
                        self.variable_types.remove(name);
                    }
                }

                // Else branch
                self.builder.position_at_end(else_block);
                let else_val = self.generate_hir_expr(else_branch)?;
                let else_val_coerced = self.coerce_to_llvm_type(else_val, result_type)?;
                let else_actual_block = self.builder.get_insert_block().unwrap();
                self.builder.build_unconditional_branch(merge_block)?;

                // Merge - create phi node for the result
                self.builder.position_at_end(merge_block);

                let phi = self.builder.build_phi(result_type, "if_result")?;
                phi.add_incoming(&[
                    (&then_val_coerced, then_actual_block),
                    (&else_val_coerced, else_actual_block),
                ]);

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
                // Check if this is an error member access (e.g., SampleError.CodegenError)
                if matches!(ty, Type::Error) {
                    let error_name = match object.as_ref() {
                        hir::HirExpr::Ident(name, _, _) => format!("{}.{}", name, member),
                        _ => member.clone(),
                    };
                    self.set_last_error_message(&error_name)?;

                    // For error member access, return a non-zero i64 value to indicate an error.
                    // This works because void! functions return i64 in LLVM, and other functions
                    // that return error types will handle the error appropriately.
                    return Ok(self
                        .context
                        .i64_type()
                        .const_int(1, false)
                        .as_basic_value_enum()
                        .into());
                }

                // Check if this is an enum variant access
                if let hir::HirExpr::Ident(obj_name, _, _) = object.as_ref() {
                    if let Some(variants) = self.enum_variants.get(obj_name) {
                        if let Some(&idx) = variants.get(member) {
                            return Ok(self.context.i64_type().const_int(idx as u64, false).into());
                        } else {
                            return Err(
                                format!("Enum variant not found: {}.{}", obj_name, member).into()
                            );
                        }
                    }
                }

                // For member access, we need to get the struct and extract the field
                // First check if object is a simple identifier - we can handle it specially

                // Get struct name and field index from the object if it's an identifier
                let (struct_name, field_idx) = if let hir::HirExpr::Ident(obj_name, _obj_ty, _) =
                    object.as_ref()
                {
                    let var_ty = match self.variable_types.get(obj_name) {
                        Some(ty) => ty.clone(),
                        None => return Err(format!("Variable type not found: {}", obj_name).into()),
                    };

                    let struct_name = match &var_ty {
                        Type::Pointer(inner) => {
                            if let Type::Custom { name, .. } = &**inner {
                                name.clone()
                            } else {
                                return Err("Member access on non-custom pointer type".into());
                            }
                        }
                        Type::Custom { name, .. } => name.clone(),
                        _ => return Err("Member access on non-struct type".into()),
                    };

                    let field_idx = self
                        .struct_field_indices
                        .get(&struct_name)
                        .and_then(|fields| fields.get(member))
                        .copied()
                        .ok_or_else(|| {
                            format!("Field '{}' not found in struct '{}'", member, struct_name)
                        })?;

                    (Some(struct_name), field_idx)
                } else {
                    // For non-identifier objects, fall back to numeric index (legacy behavior)
                    (None, member.parse().unwrap_or(0))
                };

                let extracted = if let hir::HirExpr::Ident(obj_name, _obj_ty, _) = object.as_ref() {
                    // Find the variable's allocation pointer
                    let alloca_ptr = self
                        .variables
                        .get(obj_name)
                        .or_else(|| self.const_variables.get(obj_name))
                        .copied()
                        .ok_or_else(|| format!("Variable not found: {}", obj_name))?;
                    let var_ty = self.variable_types.get(obj_name).unwrap();

                    match var_ty {
                        Type::Pointer(inner) => {
                            // Variable holds a pointer to a struct
                            if let Type::Custom {
                                name: struct_name, ..
                            } = &**inner
                            {
                                if let Some(struct_type) = self.context.get_struct_type(struct_name)
                                {
                                    // Load the pointer from alloca
                                    let opaque_ptr_type =
                                        self.context.ptr_type(inkwell::AddressSpace::default());
                                    let struct_ptr = self
                                        .builder
                                        .build_load(opaque_ptr_type, alloca_ptr, "deref_ptr")?
                                        .into_pointer_value();
                                    // GEP into the field
                                    let field_ptr = self
                                        .builder
                                        .build_struct_gep(
                                            struct_type,
                                            struct_ptr,
                                            field_idx,
                                            "field_ptr",
                                        )
                                        .map_err(|e| e.to_string())?;
                                    // Load the field value
                                    let field_type =
                                        struct_type.get_field_type_at_index(field_idx).unwrap();
                                    self.builder.build_load(field_type, field_ptr, member)?
                                } else {
                                    // Struct type not found, load as i64 fallback
                                    self.context
                                        .i64_type()
                                        .const_int(0, false)
                                        .as_basic_value_enum()
                                        .into()
                                }
                            } else {
                                // Pointer to non-struct, load it
                                let opaque_ptr_type =
                                    self.context.ptr_type(inkwell::AddressSpace::default());
                                self.builder
                                    .build_load(opaque_ptr_type, alloca_ptr, obj_name)?
                            }
                        }
                        Type::Custom {
                            name: struct_name, ..
                        } => {
                            // Variable holds a struct directly
                            if let Some(struct_type) = self.context.get_struct_type(struct_name) {
                                let field_ptr = self
                                    .builder
                                    .build_struct_gep(
                                        struct_type,
                                        alloca_ptr,
                                        field_idx,
                                        "field_ptr",
                                    )
                                    .map_err(|e| e.to_string())?;
                                let field_type =
                                    struct_type.get_field_type_at_index(field_idx).unwrap();
                                self.builder.build_load(field_type, field_ptr, member)?
                            } else {
                                // Fallback: load whole struct then extractvalue
                                let llvm_ty = self.llvm_type(var_ty);
                                let struct_val =
                                    self.builder.build_load(llvm_ty, alloca_ptr, obj_name)?;
                                self.builder.build_extract_value(
                                    struct_val.into_struct_value(),
                                    field_idx,
                                    member,
                                )?
                            }
                        }
                        _ => {
                            // Non-struct or non-pointer — load raw
                            let llvm_ty = self.llvm_type(var_ty);
                            self.builder.build_load(llvm_ty, alloca_ptr, obj_name)?
                        }
                    }
                } else {
                    // Object is not a simple identifier - evaluate it and extract from resulting value
                    let obj_val = self.generate_hir_expr(object)?;
                    match obj_val.get_type() {
                        BasicTypeEnum::StructType(_) => self.builder.build_extract_value(
                            obj_val.into_struct_value(),
                            field_idx,
                            member,
                        )?,
                        _ => {
                            // Pointer or scalar fallback - return a zeroinitializer as i64
                            self.context
                                .i64_type()
                                .const_int(0, false)
                                .as_basic_value_enum()
                                .into()
                        }
                    }
                };

                Ok(extracted.into())
            }
            hir::HirExpr::Struct {
                name, fields, ty, ..
            } => {
                // Create a struct instance
                let struct_type = self.llvm_type(ty).into_struct_type();
                let struct_name = match ty {
                    Type::Custom { name, .. } => name.clone(),
                    _ => name.clone(),
                };

                // Initialize with undefined value
                let mut struct_val = struct_type.get_undef();

                // Build the struct by inserting field values dynamically
                for (field_name, field_expr) in fields {
                    let field_val = self.generate_hir_expr(field_expr)?;

                    // Look up the field index and type
                    let (field_idx, field_ty) = self
                        .struct_field_indices
                        .get(&struct_name)
                        .and_then(|m| {
                            m.get(field_name).map(|idx| {
                                // Find the AST type from the struct definition
                                let ast_ty = self
                                    .structs
                                    .get(&struct_name)
                                    .and_then(|s| s.fields.get(*idx as usize))
                                    .map(|f| f.ty.clone())
                                    .unwrap_or(Type::I64); // Fallback
                                (*idx, ast_ty)
                            })
                        })
                        .ok_or_else(|| {
                            format!(
                                "Field '{}' not found in struct '{}'",
                                field_name, struct_name
                            )
                        })?;

                    // Coerce the field value to the expected AST type
                    let coerced_val = self.coerce_type(field_val, &field_ty)?;

                    struct_val = self
                        .builder
                        .build_insert_value(
                            struct_val,
                            coerced_val,
                            field_idx,
                            &format!("{}.{}", struct_name, field_name),
                        )?
                        .into_struct_value();
                }

                Ok(struct_val.as_basic_value_enum())
            }
            hir::HirExpr::Try { expr, .. } => {
                // Try expression: evaluate expr, if error propagate it up (return error)
                // Otherwise continue with the value
                let expr_value = self.generate_hir_expr(expr)?;

                // Check if the return type is a Result type by looking at return_type
                let is_result = self
                    .return_type
                    .as_ref()
                    .map(|t| t.is_result())
                    .unwrap_or(false);

                if is_result {
                    let result_struct = match expr_value {
                        BasicValueEnum::StructValue(sv) => sv,
                        _ => return Err("Try expression requires a result value".into()),
                    };
                    let is_error = self
                        .builder
                        .build_extract_value(result_struct, 1, "is_error")?
                        .into_int_value();

                    let function = self.current_function.unwrap();
                    let error_block = self.context.append_basic_block(function, "try_error");
                    let continue_block = self.context.append_basic_block(function, "try_continue");

                    self.builder
                        .build_conditional_branch(is_error, error_block, continue_block)?;

                    // Error path - execute defer! and return error
                    self.builder.position_at_end(error_block);
                    self.execute_defer_bang_on_error()?;
                    if self.current_function_is_main() {
                        self.emit_main_error_exit()?;
                    } else {
                        let result_value = result_struct.as_basic_value_enum();
                        self.builder.build_return(Some(&result_value))?;
                    }

                    // Success path - continue with the unwrapped value
                    self.builder.position_at_end(continue_block);
                    let value = self
                        .builder
                        .build_extract_value(result_struct, 0, "try_value")?;
                    Ok(value.into())
                } else {
                    // Not a result type, just return the value
                    Ok(expr_value)
                }
            }
            hir::HirExpr::Catch {
                expr,
                error_var: _,
                body,
                span: _,
            } => {
                // Catch expression: evaluate expr, if error execute body, otherwise return value
                let expr_value = self.generate_hir_expr(expr)?;

                // The expr_value is a Result type: { inner_value, is_error }
                let result_struct = match expr_value {
                    BasicValueEnum::StructValue(sv) => sv,
                    _ => return Err("Catch expression requires a result value".into()),
                };

                // Extract both values in the current block (branch_block)
                // This MUST be done before creating the conditional branch
                let is_error = self
                    .builder
                    .build_extract_value(result_struct, 1, "is_error")?
                    .into_int_value();
                let inner_value =
                    self.builder
                        .build_extract_value(result_struct, 0, "inner_value")?;

                // Get the basic block where the branch will be created
                let branch_block = self.builder.get_insert_block().unwrap();

                // Create blocks for error and success paths
                let function = self.current_function.unwrap();
                let catch_block = self.context.append_basic_block(function, "catch_body");
                let continue_block = self.context.append_basic_block(function, "catch_continue");

                // Branch based on whether there's an error
                self.builder
                    .build_conditional_branch(is_error, catch_block, continue_block)?;

                // Error path - execute the catch body
                self.builder.position_at_end(catch_block);
                let body_value = self.generate_hir_expr(body)?;
                self.builder.build_unconditional_branch(continue_block)?;

                // Success path - phi node goes at the top
                self.builder.position_at_end(continue_block);

                // Create PHI node to select between body value and inner value
                let phi = self
                    .builder
                    .build_phi(inner_value.get_type(), "catch_result")?;
                // Incoming from: catch_block (body_value) and branch_block (inner_value)
                phi.add_incoming(&[(&body_value, catch_block), (&inner_value, branch_block)]);
                Ok(phi.as_basic_value())
            }
        }
    }

    fn generate_hir_io_println(
        &mut self,
        args: &[hir::HirExpr],
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        let printf = self.get_or_create_printf();

        if args.is_empty() {
            let empty_str = unsafe { self.builder.build_global_string("\n", "empty") }?;
            self.builder
                .build_call(printf, &[empty_str.as_basic_value_enum().into()], "")?;
        } else {
            // Check if first argument is a string literal for format string handling
            if let hir::HirExpr::String(format_str_value, _, _) = &args[0] {
                // Parse format string and handle placeholders
                let (printf_format, arg_specs) =
                    self.parse_hir_format_string(format_str_value, args.len() - 1)?;

                // Create format string for printf
                let fmt_str = unsafe { self.builder.build_global_string(&printf_format, "fmt") }?;

                // Build argument list
                let mut llvm_args: Vec<BasicMetadataValueEnum<'_>> =
                    vec![fmt_str.as_basic_value_enum().into()];

                for &(idx, kind) in &arg_specs {
                    if idx + 1 < args.len() {
                        let raw_val = self.generate_hir_expr(&args[idx + 1])?;
                        let val = self.promote_printf_arg(raw_val, kind)?;
                        llvm_args.push(val.into());
                    }
                }

                self.builder.build_call(printf, &llvm_args, "")?;
            } else {
                // Generate the format string and argument based on the argument type
                let raw_arg = self.generate_hir_expr(&args[0])?;

                // Determine the format specifier based on the type
                let (format_str, arg_val) = match raw_arg {
                    BasicValueEnum::PointerValue(_) => {
                        // String pointers use %s format
                        ("%s\n", raw_arg)
                    }
                    BasicValueEnum::FloatValue(_) => (
                        "%f\n",
                        self.promote_printf_arg(raw_arg, PrintfArgKind::Float)?,
                    ),
                    _ => (
                        "%lld\n",
                        self.promote_printf_arg(raw_arg, PrintfArgKind::Integer)?,
                    ),
                };

                let format_ptr = unsafe { self.builder.build_global_string(format_str, "fmt") }?;

                // Convert to metadata value for function call
                let format_arg = format_ptr.as_basic_value_enum();
                let arg_meta = arg_val;

                self.builder
                    .build_call(printf, &[format_arg.into(), arg_meta.into()], "")?;
            }
        }
        Ok(self.context.i64_type().const_int(0, false).into())
    }

    /// Parse format string and extract printf format and argument indices for HIR
    fn parse_hir_format_string(
        &self,
        format_str: &str,
        num_args: usize,
    ) -> CodegenResult<(String, Vec<(usize, PrintfArgKind)>)> {
        let mut result = String::new();
        let mut arg_index = 0;
        let mut chars = format_str.chars().peekable();
        let mut arg_specs: Vec<(usize, PrintfArgKind)> = Vec::new();

        while let Some(c) = chars.next() {
            if c == '{' {
                // Look for placeholder
                let mut placeholder = String::new();
                while let Some(&pc) = chars.peek() {
                    if pc == '}' {
                        chars.next();
                        break;
                    } else {
                        placeholder.push(chars.next().unwrap());
                    }
                }

                // Process placeholder
                match placeholder.as_str() {
                    "s" => {
                        // String (ASCII) - requires u8 array/slice
                        result.push_str("%s");
                        if arg_index < num_args {
                            arg_specs.push((arg_index, PrintfArgKind::String));
                            arg_index += 1;
                        }
                    }
                    "d" => {
                        // Integer
                        result.push_str("%lld");
                        if arg_index < num_args {
                            arg_specs.push((arg_index, PrintfArgKind::Integer));
                            arg_index += 1;
                        }
                    }
                    "f" => {
                        // Float
                        result.push_str("%f");
                        if arg_index < num_args {
                            arg_specs.push((arg_index, PrintfArgKind::Float));
                            arg_index += 1;
                        }
                    }
                    "x" => {
                        // Hex
                        result.push_str("%llx");
                        if arg_index < num_args {
                            arg_specs.push((arg_index, PrintfArgKind::Integer));
                            arg_index += 1;
                        }
                    }
                    "" => {
                        // Empty placeholder - just {}
                        result.push_str("%lld");
                        if arg_index < num_args {
                            arg_specs.push((arg_index, PrintfArgKind::Integer));
                            arg_index += 1;
                        }
                    }
                    _ => {
                        // Unknown placeholder - treat as error or pass through
                        result.push('{');
                        result.push_str(&placeholder);
                        result.push('}');
                    }
                }
            } else if c == '\\' {
                // Handle escape sequences
                if let Some(&next) = chars.peek() {
                    match next {
                        'n' => {
                            chars.next();
                            result.push('\n');
                        }
                        't' => {
                            chars.next();
                            result.push('\t');
                        }
                        '\\' => {
                            chars.next();
                            result.push('\\');
                        }
                        '"' => {
                            chars.next();
                            result.push('"');
                        }
                        _ => {
                            result.push(c);
                        }
                    }
                } else {
                    result.push(c);
                }
            } else {
                result.push(c);
            }
        }

        // Add newline at the end
        result.push('\n');

        Ok((result, arg_specs))
    }

    /// Generate code for is_null and is_not_null built-in functions
    fn generate_null_check_builtin(
        &mut self,
        name: &str,
        args: &[hir::HirExpr],
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        // Generate the argument (pointer value)
        let arg = self.generate_hir_expr(&args[0])?;

        // Get the pointer value
        let ptr_value = match arg {
            BasicValueEnum::PointerValue(ptr) => ptr,
            BasicValueEnum::IntValue(int_val) => {
                // Convert integer to pointer
                self.builder.build_int_to_ptr(
                    int_val,
                    self.context.ptr_type(inkwell::AddressSpace::default()),
                    "int_to_ptr",
                )?
            }
            _ => return Err("Invalid argument type for null check".into()),
        };

        // Compare with null (zero pointer)
        let null_ptr = self
            .context
            .ptr_type(inkwell::AddressSpace::default())
            .const_null();
        let is_null = self.builder.build_int_compare(
            inkwell::IntPredicate::EQ,
            ptr_value,
            null_ptr,
            "ptr_is_null",
        )?;

        // For is_null: return true if pointer equals null
        // For is_not_null: return true if pointer does not equal null
        let result = if name == "is_null" {
            is_null
        } else {
            self.builder.build_int_compare(
                inkwell::IntPredicate::NE,
                ptr_value,
                null_ptr,
                "ptr_is_not_null",
            )?
        };

        Ok(result.into())
    }

    /// Declare a struct type in LLVM
    pub fn declare_struct(&mut self, struct_def: &TypedStructDef) -> CodegenResult<()> {
        let struct_name = &struct_def.name;

        // Create struct type (always define, regardless of visibility)
        let field_types: Vec<BasicTypeEnum> = struct_def
            .fields
            .iter()
            .map(|f| self.llvm_type(&f.ty))
            .collect();

        let struct_type = self.context.opaque_struct_type(struct_name);
        struct_type.set_body(&field_types, false);

        // Build field name to index mapping
        let mut field_map = HashMap::new();
        for (idx, field) in struct_def.fields.iter().enumerate() {
            field_map.insert(field.name.clone(), idx as u32);
        }
        self.struct_field_indices
            .insert(struct_name.clone(), field_map);

        // Declare struct methods as standalone functions
        // Methods are named as StructName_methodname
        // Always declare methods regardless of struct visibility
        for method in &struct_def.methods {
            // For methods, add the struct as a first parameter (self) only if not already present
            // Check if the first parameter is 'self' (either named "self" or "Self")
            let mut param_types: Vec<Type> = Vec::new();
            param_types.extend(method.params.iter().map(|p| p.ty.clone()));

            let fn_type = self.build_function_type(&method.return_ty, &param_types, false);
            let method_name = format!("{}_{}", struct_name, method.name);
            let mangled_name = self.mangle_name(&method_name, false);
            self.module.add_function(&mangled_name, fn_type, None);
        }

        Ok(())
    }

    /// Declare an enum type in LLVM
    pub fn declare_enum(&mut self, enum_def: &EnumDef) -> CodegenResult<()> {
        let mut variant_map = HashMap::new();
        for (idx, variant) in enum_def.variants.iter().enumerate() {
            variant_map.insert(variant.name.clone(), idx as u32);
        }
        self.enum_variants
            .insert(enum_def.name.clone(), variant_map);

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
    pub fn declare_function(&mut self, fn_def: &TypedFnDef) -> CodegenResult<()> {
        let param_types: Vec<Type> = fn_def.params.iter().map(|p| p.ty.clone()).collect();
        // Debug: Print return type
        eprintln!(
            "DEBUG declare_function: name={}, return_ty={}",
            fn_def.name, fn_def.return_ty
        );
        let fn_type =
            self.build_function_type(&fn_def.return_ty, &param_types, fn_def.name == "main");
        let mangled_name = self.mangle_name(&fn_def.name, fn_def.name == "main");

        self.module.add_function(&mangled_name, fn_type, None);

        Ok(())
    }

    /// Declare a function from a stdlib/legacy ast::FnDef (for internal use by process_imports)
    fn declare_stdlib_function(&mut self, fn_def: &FnDef) -> CodegenResult<()> {
        let param_types: Vec<Type> = fn_def.params.iter().map(|p| p.ty.clone()).collect();
        let fn_type =
            self.build_function_type(&fn_def.return_ty, &param_types, fn_def.name == "main");
        let mangled_name = self.mangle_name(&fn_def.name, fn_def.name == "main");

        self.module.add_function(&mangled_name, fn_type, None);

        Ok(())
    }

    /// Declare a struct from a stdlib/legacy ast::StructDef (for internal use by process_imports)
    fn declare_stdlib_struct(&mut self, struct_def: &StructDef) -> CodegenResult<()> {
        let struct_name = &struct_def.name;

        let field_types: Vec<BasicTypeEnum> = struct_def
            .fields
            .iter()
            .map(|f| self.llvm_type(&f.ty))
            .collect();

        let struct_type = self.context.opaque_struct_type(struct_name);
        struct_type.set_body(&field_types, false);

        let mut field_map = HashMap::new();
        for (idx, field) in struct_def.fields.iter().enumerate() {
            field_map.insert(field.name.clone(), idx as u32);
        }
        self.struct_field_indices
            .insert(struct_name.clone(), field_map);

        for method in &struct_def.methods {
            let mut param_types: Vec<Type> = Vec::new();
            param_types.extend(method.params.iter().map(|p| p.ty.clone()));
            let fn_type = self.build_function_type(&method.return_ty, &param_types, false);
            let method_name = format!("{}_{}", struct_name, method.name);
            let mangled_name = self.mangle_name(&method_name, false);
            self.module.add_function(&mangled_name, fn_type, None);
        }

        Ok(())
    }

    /// Declare an external function
    pub fn declare_external_function(
        &mut self,
        fn_def: &FnDef,
        target_module: &str,
    ) -> CodegenResult<()> {
        let param_types: Vec<Type> = fn_def.params.iter().map(|p| p.ty.clone()).collect();
        let fn_type = self.build_function_type(&fn_def.return_ty, &param_types, false);
        let mangled_name = format!("{}_{}", target_module, fn_def.name);

        self.module.add_function(
            &mangled_name,
            fn_type,
            Some(inkwell::module::Linkage::External),
        );

        Ok(())
    }

    /// Declare a C library external function (FFI)
    pub fn declare_c_function(&mut self, ext_fn: &ExternalFnDef) -> CodegenResult<()> {
        let param_types: Vec<Type> = ext_fn.params.iter().map(|p| p.ty.clone()).collect();
        let fn_type = self.build_function_type(&ext_fn.return_ty, &param_types, false);

        // Use the function name directly for C functions (no mangling)
        self.module.add_function(
            &ext_fn.name,
            fn_type,
            Some(inkwell::module::Linkage::External),
        );

        Ok(())
    }

    /// Process imports and declare imported functions
    pub fn process_imports(&mut self, imports: &[(Option<String>, String)]) -> CodegenResult<()> {
        for (alias, package_name) in imports {
            let namespace = alias.as_deref().unwrap_or(package_name.as_str());
            self.imported_packages
                .insert(namespace.to_string(), package_name.clone());

            // If it's loaded in stdlib, declare its functions, structs, and enums
            if let Some(pkg) = self.stdlib.packages().get(package_name) {
                // Clone to avoid borrow issues
                let fn_defs = pkg.functions.clone();
                let ext_fns = pkg.external_functions.clone();
                let struct_defs = pkg.structs.clone();
                let enum_defs = pkg.enums.clone();

                // Declare regular functions
                for f in fn_defs {
                    self.declare_external_function(&f, package_name)?;
                }
                // Declare external C functions (FFI)
                for ext_fn in ext_fns {
                    self.declare_c_function(&ext_fn)?;
                }
                // Declare structs
                for s in struct_defs {
                    if s.visibility == Visibility::Public {
                        let mut mangled_s = s.clone();
                        mangled_s.name = format!("{}_{}", namespace, s.name);
                        self.declare_stdlib_struct(&mangled_s)?;
                    }
                }
                // Declare enums
                for e in enum_defs {
                    if e.visibility == Visibility::Public {
                        let mut mangled_e = e.clone();
                        mangled_e.name = format!("{}_{}", namespace, e.name);
                        self.declare_enum(&mangled_e)?;
                    }
                }
            }
        }
        Ok(())
    }

    /// Generate code for a function
    pub fn generate_function(&mut self, fn_def: &FnDef) -> CodegenResult<()> {
        // Get or create the function
        let function = self
            .module
            .get_function(&fn_def.name)
            .ok_or(format!("Function not declared: {}", fn_def.name))?;

        self.current_function = Some(function);
        self.return_type = Some(fn_def.return_ty.clone());

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
        // For void functions, return void
        // For void! functions, return 0 (success code)
        if fn_def.return_ty == Type::Void {
            self.builder.build_return(None)?;
        } else {
            // Non-void function without explicit return - return zero value of correct type
            let return_type = self.llvm_type(&fn_def.return_ty);
            let zero = return_type.const_zero();
            self.builder.build_return(Some(&zero))?;
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

                    // eprintln!(
                    //     "DEBUG: Processing import: namespace={}, package={}",
                    //     namespace, package_name
                    // );
                    // eprintln!(
                    //     "DEBUG: imported_packages before: {:?}",
                    //     self.imported_packages.keys().collect::<Vec<_>>()
                    // );

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

                    // Declare all functions from the loaded package
                    if let Some(pkg) = self.stdlib.packages().get(package_name) {
                        let fn_defs: Vec<FnDef> = pkg.functions.clone();
                        for fn_def in fn_defs {
                            if let Err(e) = self.declare_stdlib_function(&fn_def) {
                                return Err(format!(
                                    "Failed to declare function from package '{}': {}",
                                    package_name, e
                                )
                                .into());
                            }
                        }
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
                if !target.contains('.') && self.const_variables.contains_key(target) {
                    return Err(format!("Cannot reassign constant variable '{}'", target).into());
                }

                // Handle member access (e.g., c.age += 43)
                if target.contains('.') {
                    let parts: Vec<&str> = target.split('.').collect();
                    let obj_name = parts[0];
                    let member = parts[1];

                    let ptr = self.variables.get(obj_name).ok_or("Var not found")?.clone();
                    let var_ty = self.variable_types.get(obj_name).unwrap();

                    // Get the struct name from the variable type
                    let struct_name = match var_ty {
                        Type::Pointer(inner) => {
                            if let Type::Custom { name, .. } = &**inner {
                                name.clone()
                            } else {
                                return Err("Member assignment on non-custom pointer type".into());
                            }
                        }
                        Type::Custom { name, .. } => name.clone(),
                        _ => return Err("Member assignment on non-struct type".into()),
                    };

                    // Look up the field index
                    let field_idx = self
                        .struct_field_indices
                        .get(&struct_name)
                        .and_then(|fields| fields.get(member))
                        .copied()
                        .ok_or_else(|| {
                            format!("Field '{}' not found in struct '{}'", member, struct_name)
                        })?;

                    // Get struct pointer and type
                    let (struct_ptr, struct_type) = if let Type::Pointer(inner) = var_ty {
                        if let Type::Custom {
                            name: struct_name, ..
                        } = &**inner
                        {
                            let st = self.context.get_struct_type(struct_name).unwrap();
                            let opaque_ptr_type =
                                self.context.ptr_type(inkwell::AddressSpace::default());
                            let loaded_ptr = self
                                .builder
                                .build_load(opaque_ptr_type, ptr, "obj_ptr")?
                                .into_pointer_value();
                            (loaded_ptr, st)
                        } else {
                            return Err("Member assignment on non-custom pointer type".into());
                        }
                    } else if let Type::Custom {
                        name: struct_name, ..
                    } = var_ty
                    {
                        let st = self.context.get_struct_type(struct_name).unwrap();
                        (ptr, st)
                    } else {
                        return Err("Member assignment on non-struct type".into());
                    };

                    let field_ptr = self
                        .builder
                        .build_struct_gep(struct_type, struct_ptr, field_idx, "field_ptr")
                        .map_err(|e| e.to_string())?;
                    let field_type = struct_type.get_field_type_at_index(field_idx).unwrap();

                    let llvm_value = self.generate_expr(value)?;

                    match op {
                        AssignOp::Assign => {
                            // Coerce and store the value
                            let mut coerced_val = llvm_value;
                            if field_type.is_int_type() && llvm_value.is_int_value() {
                                coerced_val = self
                                    .builder
                                    .build_int_cast(
                                        llvm_value.into_int_value(),
                                        field_type.into_int_type(),
                                        "assign_cast",
                                    )?
                                    .into();
                            }
                            self.builder.build_store(field_ptr, coerced_val)?;
                        }
                        AssignOp::AddAssign => {
                            let current = self.builder.build_load(field_type, field_ptr, "tmp")?;
                            let result = self.builder.build_int_add(
                                current.into_int_value(),
                                llvm_value.into_int_value(),
                                "addtmp",
                            )?;
                            self.builder.build_store(field_ptr, result)?;
                        }
                        AssignOp::SubAssign => {
                            let current = self.builder.build_load(field_type, field_ptr, "tmp")?;
                            let result = self.builder.build_int_sub(
                                current.into_int_value(),
                                llvm_value.into_int_value(),
                                "subtmp",
                            )?;
                            self.builder.build_store(field_ptr, result)?;
                        }
                        AssignOp::MulAssign => {
                            let current = self.builder.build_load(field_type, field_ptr, "tmp")?;
                            let result = self.builder.build_int_mul(
                                current.into_int_value(),
                                llvm_value.into_int_value(),
                                "multmp",
                            )?;
                            self.builder.build_store(field_ptr, result)?;
                        }
                        AssignOp::DivAssign => {
                            let current = self.builder.build_load(field_type, field_ptr, "tmp")?;
                            let result = self.builder.build_int_signed_div(
                                current.into_int_value(),
                                llvm_value.into_int_value(),
                                "divtmp",
                            )?;
                            self.builder.build_store(field_ptr, result)?;
                        }
                    }

                    return Ok(());
                }

                // Simple variable assignment (non-member)
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
                    // Check if the return type is void! (Result where inner is Void)
                    // In this case, we need to return void instead of the error value
                    // because LLVM void functions cannot return a value
                    let is_void_result = self
                        .return_type
                        .as_ref()
                        .map(|t| t.is_void_result())
                        .unwrap_or(false);
                    if is_void_result {
                        self.builder.build_return(None)?;
                    } else {
                        self.builder.build_return(Some(&llvm_val))?;
                    }
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
            Stmt::For { .. } => todo!("Codegen for For loops not implemented"),
            Stmt::Switch { .. } => todo!("Codegen for Switch statements not implemented"),
            Stmt::Defer { stmt, .. } => {
                // Defer is handled at function level - collect and execute in reverse order
                // For now, just generate the deferred statement immediately
                // A proper implementation would collect defers and execute them on scope exit
                self.generate_stmt(stmt)?;
                Ok(())
            }
            Stmt::DeferBang { stmt, .. } => {
                // DeferBang is similar to Defer but only executes on error
                // For now, just generate the deferred statement immediately
                self.generate_stmt(stmt)?;
                Ok(())
            }
            Stmt::Break { label, span } => {
                // Break statement - similar handling as in HirStmt
                // For now, break just branches to the end block
                if let Some(loop_stack) = self.loop_end_blocks.last() {
                    // Find the appropriate end block
                    if let Some(target_block) = loop_stack.iter().find_map(|(block, l)| {
                        if l.as_ref() == label.as_ref() {
                            Some(*block)
                        } else if label.is_none() {
                            // If no label specified, use the innermost loop
                            Some(*block)
                        } else {
                            None
                        }
                    }) {
                        self.builder.build_unconditional_branch(target_block)?;
                    } else {
                        return Err(format!(
                            "break statement with label '{}' not found in scope at span {:?}",
                            label.as_deref().unwrap_or("none"),
                            span
                        )
                        .into());
                    }
                } else {
                    return Err(
                        format!("break statement outside of loop at span {:?}", span).into(),
                    );
                }
                Ok(())
            }
        }
    }

    /// Generate code for an expression and return its LLVM value
    fn generate_expr(&mut self, expr: &Expr) -> CodegenResult<BasicValueEnum<'ctx>> {
        match expr {
            Expr::Int(value, _) => {
                let i64_type = self.context.i64_type();
                Ok(i64_type.const_int(*value as u64, false).into())
            }
            Expr::Float(value, _) => {
                let f64_type = self.context.f64_type();
                Ok(f64_type.const_float(*value).into())
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
            Expr::Array(_, _, _) => todo!("Codegen for Array literals not implemented"),
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
            Expr::Try { expr, .. } => {
                // For now, just evaluate the expression and return its value
                self.generate_expr(expr)
            }
            Expr::Catch {
                expr,
                error_var: _,
                body: _,
                ..
            } => {
                // Catch expression: evaluate expr, if error execute body, otherwise return value
                // For now, we just evaluate the expression and ignore the catch
                // A full implementation would handle the error case
                let expr_value = self.generate_expr(expr)?;

                // For now, just return the expression value
                // A full implementation would:
                // 1. Evaluate expr
                // 2. Check if it's an error
                // 3. If error, bind to error_var and execute body
                // 4. If success, return the value
                Ok(expr_value)
            }
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
            BinaryOp::BitAnd => self
                .builder
                .build_and(
                    left_val.into_int_value(),
                    right_val.into_int_value(),
                    "bitand",
                )?
                .into(),
            BinaryOp::BitOr => self
                .builder
                .build_or(
                    left_val.into_int_value(),
                    right_val.into_int_value(),
                    "bitor",
                )?
                .into(),
            BinaryOp::BitXor => self
                .builder
                .build_xor(
                    left_val.into_int_value(),
                    right_val.into_int_value(),
                    "bitxor",
                )?
                .into(),
            BinaryOp::Shl => self
                .builder
                .build_left_shift(left_val.into_int_value(), right_val.into_int_value(), "shl")?
                .into(),
            BinaryOp::Shr => self
                .builder
                .build_right_shift(
                    left_val.into_int_value(),
                    right_val.into_int_value(),
                    false, // logical shift
                    "shr",
                )?
                .into(),
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
        // Handle std library and namespaced function calls
        let (mangled_name, _is_std) = if let Some(ns) = namespace {
            // Resolve alias to actual package name
            let actual_package = self
                .imported_packages
                .get(ns)
                .map(|s| s.as_str())
                .unwrap_or(ns);

            // Special handling for io.println
            if actual_package == "io" && name == "println" {
                return self.generate_io_println(args);
            }

            (format!("{}_{}", actual_package, name), true)
        } else {
            // Internal call
            if name == "main" {
                ("main".to_string(), false)
            } else {
                (format!("{}_{}", self.module_name, name), false)
            }
        };

        // Try to get the function - first try mangled name, then try original name
        // This supports both Lang functions (mangled) and C functions (original name)
        let function = self
            .module
            .get_function(&mangled_name)
            .or_else(|| {
                // Fallback to demangled/original name if not found
                // This is needed for C library calls (like exit, open, etc.)
                self.module.get_function(name)
            })
            .ok_or(format!(
                "Function not found: {} (original: {})",
                mangled_name, name
            ))?;

        // Generate arguments
        let mut llvm_args = Vec::new();
        for arg in args {
            let val = self.generate_expr(arg)?;
            llvm_args.push(BasicMetadataValueEnum::from(val));
        }

        let call_site = self.builder.build_call(function, &llvm_args, "call_tmp")?;

        // Handle return value
        match call_site.try_as_basic_value() {
            inkwell::values::ValueKind::Basic(val) => Ok(val),
            _ => Ok(self.context.i64_type().const_int(0, false).into()), // Default value for void or error
        }
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
            // Generate first argument (format string)
            let format_arg = &args[0];

            // Check if first argument is a string literal
            if let Expr::String(format_str, _) = format_arg {
                // Parse format string and handle placeholders
                let (printf_format, arg_specs) =
                    self.parse_format_string(format_str, args.len() - 1)?;

                // Create format string for printf
                let fmt_str = unsafe { self.builder.build_global_string(&printf_format, "fmt") }?;

                // Build argument list
                let mut llvm_args: Vec<BasicMetadataValueEnum<'_>> =
                    vec![fmt_str.as_basic_value_enum().into()];

                for &(idx, kind) in &arg_specs {
                    let raw_val = self.generate_expr(&args[idx + 1])?;
                    let val = self.promote_printf_arg(raw_val, kind)?;
                    llvm_args.push(val.into());
                }

                self.builder.build_call(printf, &llvm_args, "")?;
            } else {
                // Not a string literal - generate as before (simple print)
                let raw_arg = self.generate_expr(&args[0])?;
                let arg_type = raw_arg.get_type();

                if let BasicTypeEnum::PointerType(_) = arg_type {
                    // It's a string pointer - use %s format
                    let fmt_str = unsafe { self.builder.build_global_string("%s\n", "fmt") }?;
                    let metadata_arg: inkwell::values::BasicMetadataValueEnum<'_> =
                        fmt_str.as_basic_value_enum().into();
                    let arg_metadata: inkwell::values::BasicMetadataValueEnum<'_> = raw_arg.into();
                    self.builder
                        .build_call(printf, &[metadata_arg, arg_metadata], "")?;
                } else if raw_arg.is_float_value() {
                    let fmt_str = unsafe { self.builder.build_global_string("%f\n", "fmt") }?;
                    let metadata_arg: inkwell::values::BasicMetadataValueEnum<'_> =
                        fmt_str.as_basic_value_enum().into();
                    let arg_metadata: inkwell::values::BasicMetadataValueEnum<'_> = self
                        .promote_printf_arg(raw_arg, PrintfArgKind::Float)?
                        .into();
                    self.builder
                        .build_call(printf, &[metadata_arg, arg_metadata], "")?;
                } else {
                    // It's a numeric value - use %lld format
                    let fmt_str = unsafe { self.builder.build_global_string("%lld\n", "fmt") }?;
                    let metadata_arg: inkwell::values::BasicMetadataValueEnum<'_> =
                        fmt_str.as_basic_value_enum().into();
                    let arg_metadata: inkwell::values::BasicMetadataValueEnum<'_> = self
                        .promote_printf_arg(raw_arg, PrintfArgKind::Integer)?
                        .into();
                    self.builder
                        .build_call(printf, &[metadata_arg, arg_metadata], "")?;
                }
            }
        }

        Ok(self.context.i64_type().const_int(0, false).into())
    }

    /// Parse format string and extract printf format and argument indices
    /// Format placeholders: {s} = string (u8 array), {d} = integer, {f} = float, {x} = hex
    fn parse_format_string(
        &self,
        format_str: &str,
        num_args: usize,
    ) -> CodegenResult<(String, Vec<(usize, PrintfArgKind)>)> {
        let mut result = String::new();
        let mut arg_index = 0;
        let mut chars = format_str.chars().peekable();
        let mut arg_specs: Vec<(usize, PrintfArgKind)> = Vec::new();

        while let Some(c) = chars.next() {
            if c == '{' {
                // Look for placeholder
                let mut placeholder = String::new();
                while let Some(&pc) = chars.peek() {
                    if pc == '}' {
                        chars.next();
                        break;
                    } else {
                        placeholder.push(chars.next().unwrap());
                    }
                }

                // Process placeholder
                match placeholder.as_str() {
                    "s" => {
                        // String (ASCII) - requires u8 array/slice
                        result.push_str("%s");
                        if arg_index < num_args {
                            arg_specs.push((arg_index, PrintfArgKind::String));
                            arg_index += 1;
                        }
                    }
                    "d" => {
                        // Integer
                        result.push_str("%lld");
                        if arg_index < num_args {
                            arg_specs.push((arg_index, PrintfArgKind::Integer));
                            arg_index += 1;
                        }
                    }
                    "f" => {
                        // Float
                        result.push_str("%f");
                        if arg_index < num_args {
                            arg_specs.push((arg_index, PrintfArgKind::Float));
                            arg_index += 1;
                        }
                    }
                    "x" => {
                        // Hex
                        result.push_str("%llx");
                        if arg_index < num_args {
                            arg_specs.push((arg_index, PrintfArgKind::Integer));
                            arg_index += 1;
                        }
                    }
                    "" => {
                        // Empty placeholder - just {}
                        result.push_str("%lld");
                        if arg_index < num_args {
                            arg_specs.push((arg_index, PrintfArgKind::Integer));
                            arg_index += 1;
                        }
                    }
                    _ => {
                        // Unknown placeholder - treat as error or pass through
                        result.push('{');
                        result.push_str(&placeholder);
                        result.push('}');
                    }
                }
            } else if c == '\\' {
                // Handle escape sequences
                if let Some(&next) = chars.peek() {
                    match next {
                        'n' => {
                            chars.next();
                            result.push('\n');
                        }
                        't' => {
                            chars.next();
                            result.push('\t');
                        }
                        '\\' => {
                            chars.next();
                            result.push('\\');
                        }
                        '"' => {
                            chars.next();
                            result.push('"');
                        }
                        _ => {
                            result.push(c);
                        }
                    }
                } else {
                    result.push(c);
                }
            } else {
                result.push(c);
            }
        }

        // Add newline at the end
        result.push('\n');

        Ok((result, arg_specs))
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

    /// Coerce a value to the expected type (e.g. wrap in Option/Result, downcast integers)
    fn coerce_type(
        &self,
        val: BasicValueEnum<'ctx>,
        expected_ty: &Type,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        let expected_llvm_ty = self.llvm_type(expected_ty);
        if val.get_type() == expected_llvm_ty {
            return Ok(val);
        }

        match expected_ty {
            Type::Option(_) | Type::Result(_) => {
                let struct_type = expected_llvm_ty.into_struct_type();
                let mut struct_val = struct_type.get_undef();
                let field_type = struct_type.get_field_types()[0];
                let mut inner_val = val;
                if inner_val.get_type() != field_type {
                    if field_type.is_int_type() && inner_val.is_int_value() {
                        inner_val = self
                            .builder
                            .build_int_cast(
                                inner_val.into_int_value(),
                                field_type.into_int_type(),
                                "cast",
                            )?
                            .into();
                    } else if field_type.is_float_type() && inner_val.is_float_value() {
                        inner_val = self
                            .builder
                            .build_float_cast(
                                inner_val.into_float_value(),
                                field_type.into_float_type(),
                                "cast",
                            )?
                            .into();
                    } else if inner_val.is_struct_value() {
                        return Ok(struct_type.const_zero().into());
                    }
                }
                struct_val = self
                    .builder
                    .build_insert_value(struct_val, inner_val, 0, "ret_val")?
                    .into_struct_value();

                let flag_val = if matches!(expected_ty, Type::Option(_)) {
                    self.context.bool_type().const_int(1, false) // is_valid = true
                } else {
                    self.context.bool_type().const_int(0, false) // is_error = false
                };
                struct_val = self
                    .builder
                    .build_insert_value(struct_val, flag_val, 1, "flag")?
                    .into_struct_value();

                return Ok(struct_val.into());
            }
            _ => {
                // Simple integer casting
                if expected_llvm_ty.is_int_type() && val.is_int_value() {
                    return Ok(self
                        .builder
                        .build_int_cast(
                            val.into_int_value(),
                            expected_llvm_ty.into_int_type(),
                            "cast",
                        )?
                        .into());
                }
            }
        }

        Ok(val)
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
            Type::F32 => self.context.f32_type().into(),
            Type::F64 => self.context.f64_type().into(),
            Type::ImmInt => self.context.i64_type().into(),
            Type::ImmFloat => self.context.f64_type().into(),
            Type::Bool => self.context.bool_type().into(),
            Type::RawPtr => self
                .context
                .ptr_type(inkwell::AddressSpace::default())
                .into(),
            Type::SelfType => self.context.i64_type().into(), // TODO: Resolve to actual struct type
            Type::Pointer(_) => self
                .context
                .ptr_type(inkwell::AddressSpace::default())
                .into(),
            Type::Void => self.context.i64_type().into(), // For now, keep i64 for void to match other parts
            Type::Error => self.context.i64_type().into(),
            Type::Option(inner) => {
                let bool_type = self.context.bool_type();
                let value_type = self.llvm_type(inner);
                self.context
                    .struct_type(&[value_type.into(), bool_type.into()], false)
                    .into()
            }
            Type::Result(inner) => {
                // Result<T> is represented same as Option<T> for now: { value, is_error }
                let bool_type = self.context.bool_type();
                let value_type = self.llvm_type(inner);
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
            Type::Custom { name, .. } => {
                // Look up struct type in context
                if let Some(st) = self.context.get_struct_type(name) {
                    st.into()
                } else {
                    // Fallback to i64 if not found (might be declared later)
                    self.context.i64_type().into()
                }
            }
            Type::GenericParam(_) => {
                // For generics, we'll just use i64 for now
                self.context.i64_type().into()
            }
            Type::Array { size, element_type } => {
                // Create array type: [size x element_type]
                let element_llvm = self.llvm_type(element_type);
                // Use the element type's array_type method
                match size {
                    Some(n) => element_llvm.array_type(*n as u32).into(),
                    None => {
                        // For dynamic arrays, use a pointer for now
                        self.context
                            .ptr_type(inkwell::AddressSpace::default())
                            .into()
                    }
                }
            }
            Type::Function { .. } => {
                // Function type is represented as a pointer to the function
                self.context
                    .ptr_type(inkwell::AddressSpace::default())
                    .into()
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
            BasicTypeEnum::FloatType(ft) => {
                if ft == &self.context.f32_type() {
                    Type::F32
                } else {
                    Type::F64
                }
            }
            BasicTypeEnum::PointerType(_) => Type::RawPtr,
            BasicTypeEnum::StructType(st) => Type::Tuple(
                st.get_field_types()
                    .iter()
                    .map(|field_ty| self.llvm_type_to_lang(field_ty))
                    .collect(),
            ),
            BasicTypeEnum::ArrayType(at) => Type::Array {
                size: Some(at.len() as usize),
                element_type: Box::new(self.llvm_type_to_lang(&at.get_element_type())),
            },
            BasicTypeEnum::VectorType(_) => Type::I64, // Default vector to i64
            BasicTypeEnum::ScalableVectorType(_) => Type::I64, // Default scalable vector to i64
        }
    }

    /// Coerce a BasicValueEnum to a specific LLVM type
    fn coerce_to_llvm_type(
        &self,
        val: BasicValueEnum<'ctx>,
        target_ty: inkwell::types::BasicTypeEnum<'ctx>,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        let val_ty = val.get_type();
        if val_ty == target_ty {
            return Ok(val);
        }

        // Handle int to int coercion
        if let BasicTypeEnum::IntType(target_int) = target_ty {
            if let BasicValueEnum::IntValue(int_val) = val {
                if int_val.get_type() != target_int {
                    return Ok(self
                        .builder
                        .build_int_cast(int_val, target_int, "coerce_int")?
                        .into());
                }
            }
        }

        // Handle float to float coercion
        if let BasicTypeEnum::FloatType(target_float) = target_ty {
            if let BasicValueEnum::FloatValue(float_val) = val {
                if float_val.get_type() != target_float {
                    return Ok(self
                        .builder
                        .build_float_cast(float_val, target_float, "coerce_float")?
                        .into());
                }
            }
        }

        // Handle int to float coercion
        if let BasicTypeEnum::FloatType(target_float) = target_ty {
            if let BasicValueEnum::IntValue(int_val) = val {
                return Ok(self
                    .builder
                    .build_unsigned_int_to_float(int_val, target_float, "coerce_int_to_float")?
                    .into());
            }
        }

        // Handle float to int coercion
        if let BasicTypeEnum::IntType(target_int) = target_ty {
            if let BasicValueEnum::FloatValue(float_val) = val {
                return Ok(self
                    .builder
                    .build_float_to_unsigned_int(float_val, target_int, "coerce_float_to_int")?
                    .into());
            }
        }

        // Default: just return the original value if we can't coerce
        Ok(val)
    }

    /// Print the generated LLVM IR
    pub fn print_ir(&self) -> String {
        self.module.print_to_string().to_string()
    }
}
