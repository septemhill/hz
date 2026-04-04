use super::*;

#[allow(unused)]
impl<'ctx> CodeGenerator<'ctx> {
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

        let mut intrinsics = HashMap::new();
        for intrinsic in crate::builtin::get_intrinsics() {
            intrinsics.insert(intrinsic.name().to_string(), intrinsic);
        }

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
            loop_continue_blocks: Vec::new(),
            stdlib,
            imported_packages: HashMap::new(),
            module_name: module_name.to_string(),
            enum_variants: HashMap::new(),
            struct_field_indices: HashMap::new(),
            structs,
            enums,
            errors,
            external_function_names: std::collections::HashSet::new(),
            intrinsics,
        })
    }

    /// Push a new scope onto the defer stack
    pub(super) fn push_defer_scope(&mut self) {
        self.defer_stack.push(Vec::new());
    }

    /// Pop the current scope and execute all defers in LIFO order
    pub(super) fn pop_defer_scope(&mut self) -> CodegenResult<()> {
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
    pub(super) fn add_defer(&mut self, stmt: hir::HirStmt) {
        if let Some(current_scope) = self.defer_stack.last_mut() {
            current_scope.push(stmt);
        }
    }

    /// Push a new scope onto the defer! stack
    pub(super) fn push_defer_bang_scope(&mut self) {
        self.defer_bang_stack.push(Vec::new());
    }

    /// Pop the current scope and execute all defer!s in LIFO order (only on error)
    pub(super) fn pop_defer_bang_scope(&mut self) -> CodegenResult<()> {
        if let Some(defers) = self.defer_bang_stack.pop() {
            // Execute defer!s in reverse order (LIFO)
            for defer_stmt in defers.iter().rev() {
                self.generate_hir_stmt(defer_stmt)?;
            }
        }
        Ok(())
    }

    /// Add a defer! statement to the current scope
    pub(super) fn add_defer_bang(&mut self, stmt: hir::HirStmt) {
        if let Some(current_scope) = self.defer_bang_stack.last_mut() {
            current_scope.push(stmt);
        }
    }

    /// Execute defer! statements (cleanup on error) - called when an error is detected
    pub(super) fn execute_defer_bang_on_error(&mut self) -> CodegenResult<()> {
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
        // First, declare all external functions
        for ext_fn in &program.external_functions {
            self.external_function_names.insert(ext_fn.name.clone());
            let param_types: Vec<Type> = ext_fn.params.iter().map(|(_, ty)| ty.clone()).collect();
            let fn_type = self.build_function_type(&ext_fn.return_ty, &param_types, false);
            // Use the original name for external functions
            self.module.add_function(&ext_fn.name, fn_type, None);
        }

        // Generate code for each function in HIR
        for hir_fn in &program.functions {
            self.generate_hir_function(hir_fn)?;
        }

        Ok(())
    }

    /// Convert a condition value to i1 (boolean) for branching
    /// If already i1, returns as-is; otherwise converts i64 to i1
    /// Handles optional types (represented as struct { value, valid_flag })
    pub(super) fn condition_to_i1(
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
    pub(super) fn mangle_name(&self, name: &str, is_main: bool) -> String {
        if is_main || name == "main" {
            return "main".to_string();
        }

        // Check if the function is external in the symbol table
        if self.external_function_names.contains(name) {
            return name.to_string();
        }

        format!("{}_{}", self.module_name, name)
    }

    /// Get the monomorphized struct name from a generic struct type.
    /// This looks up the struct in self.structs to find the actual instantiated type name.
    pub(super) fn get_monomorphized_struct_name(&self, type_name: &str) -> String {
        eprintln!(
            "DEBUG get_monomorphized_struct_name: type_name={}",
            type_name
        );
        // Check if the struct exists in our structs map with exact name
        if let Some(struct_def) = self.structs.get(type_name) {
            // If the struct has no generic params, it's already monomorphized
            if struct_def.generic_params.is_empty() {
                return type_name.to_string();
            }
        }

        // If not found exactly or has generic params, try to find a matching monomorphized struct
        // The struct name might be like "Compose_i32" which is already mangled
        // We look for structs that have the original type name as a suffix after _
        for (struct_key, struct_def) in &self.structs {
            // Check if this is a monomorphized version (has no generic params but name matches pattern)
            if struct_def.generic_params.is_empty() && struct_key != type_name {
                // Check if struct_key ends with _type_name
                if struct_key.ends_with(&format!("_{}", type_name)) {
                    return struct_key.clone();
                }
            }
        }

        // Return original name if no match found
        type_name.to_string()
    }

    pub(super) fn llvm_function_return_type(
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

    pub(super) fn llvm_param_type(&self, param_ty: &Type) -> BasicMetadataTypeEnum<'ctx> {
        match param_ty {
            Type::VarArgsPack(_) => self
                .context
                .ptr_type(inkwell::AddressSpace::default())
                .into(),
            _ => self.llvm_type(param_ty).into(),
        }
    }

    pub(super) fn is_pointer_varargs_pack(&self, ty: &Type) -> bool {
        matches!(ty, Type::VarArgsPack(_))
    }

    pub(super) fn add_varargs_param_attributes_to_function(
        &self,
        function: FunctionValue<'ctx>,
        param_tys: &[Type],
    ) {
        let readonly_kind = Attribute::get_named_enum_kind_id("readonly");
        let nocapture_kind = Attribute::get_named_enum_kind_id("nocapture");

        for (idx, ty) in param_tys.iter().enumerate() {
            if !self.is_pointer_varargs_pack(ty) {
                continue;
            }

            let readonly = self.context.create_enum_attribute(readonly_kind, 0);
            let nocapture = self.context.create_enum_attribute(nocapture_kind, 0);
            function.add_attribute(AttributeLoc::Param(idx as u32), readonly);
            function.add_attribute(AttributeLoc::Param(idx as u32), nocapture);
        }
    }

    pub(super) fn add_varargs_param_attributes_to_callsite(
        &self,
        call_site: inkwell::values::CallSiteValue<'ctx>,
        param_tys: &[Type],
    ) {
        let readonly_kind = Attribute::get_named_enum_kind_id("readonly");
        let nocapture_kind = Attribute::get_named_enum_kind_id("nocapture");

        for (idx, ty) in param_tys.iter().enumerate() {
            if !self.is_pointer_varargs_pack(ty) {
                continue;
            }

            let readonly = self.context.create_enum_attribute(readonly_kind, 0);
            let nocapture = self.context.create_enum_attribute(nocapture_kind, 0);
            call_site.add_attribute(AttributeLoc::Param(idx as u32), readonly);
            call_site.add_attribute(AttributeLoc::Param(idx as u32), nocapture);
        }
    }

    pub(super) fn build_call_arg_from_hir_expr(
        &mut self,
        arg_expr: &hir::HirExpr,
    ) -> CodegenResult<BasicMetadataValueEnum<'ctx>> {
        let arg_val = self.generate_hir_expr(arg_expr)?;

        if self.is_pointer_varargs_pack(arg_expr.ty()) {
            let pack_type = self.llvm_type(arg_expr.ty());
            let pack_ptr = self.builder.build_alloca(pack_type, "varargs_pack")?;
            self.builder.build_store(pack_ptr, arg_val)?;
            Ok(pack_ptr.into())
        } else {
            Ok(arg_val.into())
        }
    }

    pub(super) fn default_llvm_return_value(
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

    pub(super) fn current_function_is_main(&self) -> bool {
        self.current_function
            .map(|function| function.get_name().to_str().ok() == Some("main"))
            .unwrap_or(false)
    }

    pub(super) fn current_block_has_terminator(&self) -> bool {
        self.builder
            .get_insert_block()
            .and_then(|block| block.get_terminator())
            .is_some()
    }

    pub(super) fn get_or_create_printf(&self) -> FunctionValue<'ctx> {
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

    pub(super) fn get_or_create_last_error_global(&self) -> GlobalValue<'ctx> {
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

    pub(super) fn clear_last_error_message(&mut self) -> CodegenResult<()> {
        let ptr_ty = self.context.ptr_type(inkwell::AddressSpace::default());
        let last_error = self.get_or_create_last_error_global();
        self.builder
            .build_store(last_error.as_pointer_value(), ptr_ty.const_null())?;
        Ok(())
    }

    pub(super) fn set_last_error_message(&mut self, message: &str) -> CodegenResult<()> {
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

    pub(super) fn emit_last_error_message(&mut self) -> CodegenResult<()> {
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

    pub(super) fn emit_main_error_exit(&mut self) -> CodegenResult<()> {
        self.emit_last_error_message()?;
        self.builder
            .build_return(Some(&self.context.i64_type().const_int(1, false)))?;
        Ok(())
    }

    pub(super) fn hir_expr_type<'a>(&self, expr: &'a hir::HirExpr) -> &'a Type {
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
            hir::HirExpr::Try { ty, .. } => ty,
            hir::HirExpr::Catch { ty, .. } => ty,
            hir::HirExpr::Cast { ty, .. } => ty,
            hir::HirExpr::Dereference { ty, .. }
            | hir::HirExpr::Index { ty, .. }
            | hir::HirExpr::Intrinsic { ty, .. }
            | hir::HirExpr::TypeLiteral(_, ty, _) => ty,
        }
    }

    pub(super) fn build_typed_int_constant(&self, value: i64, ty: &Type) -> BasicValueEnum<'ctx> {
        match self.llvm_type(ty) {
            BasicTypeEnum::IntType(int_ty) => int_ty
                .const_int(value as u64, ty.is_signed_integer())
                .into(),
            _ => self.context.i64_type().const_int(value as u64, true).into(),
        }
    }

    pub(super) fn promote_printf_arg(
        &self,
        value: BasicValueEnum<'ctx>,
        kind: PrintfArgKind,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        match kind {
            PrintfArgKind::String => {
                if value.is_struct_value() {
                    let sv = value.into_struct_value();
                    if sv.get_type().get_field_types().len() == 2
                        && sv.get_type().get_field_types()[0].is_pointer_type()
                        && sv.get_type().get_field_types()[1].is_int_type()
                    {
                        return Ok(self.builder.build_extract_value(sv, 0, "slice_ptr")?);
                    }
                }
                Ok(value)
            }
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
            PrintfArgKind::Boolean => {
                // For boolean, we need to convert to string "true"/"false"
                // This is handled specially in io.rs code generation
                // Here we just ensure it's properly extended to i64 for the conditional
                if value.is_int_value() {
                    let int_val = value.into_int_value();
                    if int_val.get_type().get_bit_width() == 1 {
                        Ok(self
                            .builder
                            .build_int_z_extend(int_val, self.context.i64_type(), "printf_bool")?
                            .into())
                    } else {
                        Ok(value)
                    }
                } else {
                    Ok(value)
                }
            }
            PrintfArgKind::Char => {
                // For char, printf expects int (i32), so extend i8/u8 to i32
                if value.is_int_value() {
                    let int_val = value.into_int_value();
                    if int_val.get_type().get_bit_width() == 8 {
                        // Zero extend for both signed and unsigned chars
                        Ok(self
                            .builder
                            .build_int_z_extend(int_val, self.context.i32_type(), "printf_char")?
                            .into())
                    } else {
                        Ok(value)
                    }
                } else {
                    Ok(value)
                }
            }
        }
    }

    /// Build function type from return type and parameter types
    pub(super) fn build_function_type(
        &self,
        return_ty: &Type,
        param_tys: &[Type],
        is_main: bool,
    ) -> inkwell::types::FunctionType<'ctx> {
        let mut param_types: Vec<BasicMetadataTypeEnum> =
            param_tys.iter().map(|p| self.llvm_param_type(p)).collect();

        // Check if the last parameter is varargs (C-style)
        let is_varargs = param_tys.last().is_some_and(|t| matches!(t, Type::VarArgs));

        // If it's C-style varargs, remove the varargs marker from the LLVM parameter list
        if is_varargs {
            param_types.pop();
        }

        match self.llvm_function_return_type(return_ty, is_main) {
            Some(return_type) => return_type.fn_type(&param_types, is_varargs),
            None => self.context.void_type().fn_type(&param_types, is_varargs),
        }
    }

    /// Generate code for an HIR function
    pub(super) fn generate_hir_function(&mut self, hir_fn: &hir::HirFn) -> CodegenResult<()> {
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

            if self.is_pointer_varargs_pack(ty) {
                self.variables
                    .insert(name.clone(), param_value.into_pointer_value());
            } else {
                let llvm_type = self.llvm_type(ty);
                let alloca = self.builder.build_alloca(llvm_type, name)?;
                self.builder.build_store(alloca, param_value)?;
                self.variables.insert(name.clone(), alloca);
            }

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
}
