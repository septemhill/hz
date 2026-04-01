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

mod ast_codegen;
mod core;
mod declarations;
mod hir_codegen;

#[cfg(test)]
mod tests;

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

    // Stack of loop continue blocks for continue statements
    // Each entry is a vector of (continue_block, label) pairs
    loop_continue_blocks: Vec<Vec<(inkwell::basic_block::BasicBlock<'ctx>, Option<String>)>>,

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

    // Intrinsics
    pub intrinsics: std::collections::HashMap<String, std::rc::Rc<dyn crate::builtin::Intrinsic>>,
}

/// Result of code generation
pub type CodegenResult<T> = Result<T, Box<dyn Error>>;

#[derive(Clone, Copy)]
enum PrintfArgKind {
    String,
    Integer,
    Float,
    Boolean,
}
