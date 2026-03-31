use super::*;

#[allow(unused)]
impl<'ctx> CodeGenerator<'ctx> {
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
    pub(super) fn generate_stmt(&mut self, stmt: &Stmt) -> CodegenResult<()> {
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
                        _ => todo!("Compound assignment {:?} not implemented in AST codegen", op),
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
                    _ => todo!("Compound assignment {:?} not implemented in AST codegen", op),
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
            Stmt::Continue { label, span } => {
                // Continue statement - similar handling as in HirStmt
                // Continue branches to the condition block to start the next iteration
                if let Some(loop_stack) = self.loop_continue_blocks.last() {
                    // Find the appropriate continue block
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
                            "continue statement with label '{}' not found in scope at span {:?}",
                            label.as_deref().unwrap_or("none"),
                            span
                        )
                        .into());
                    }
                } else {
                    return Err(
                        format!("continue statement outside of loop at span {:?}", span).into(),
                    );
                }
                Ok(())
            }
        }
    }

    /// Generate code for an expression and return its LLVM value
    pub(super) fn generate_expr(&mut self, expr: &Expr) -> CodegenResult<BasicValueEnum<'ctx>> {
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
            Expr::Cast {
                target_type, expr, ..
            } => {
                // Generate the expression first
                let expr_value = self.generate_expr(expr)?;
                // Then convert to target type
                self.generate_cast(expr_value, target_type)
            }
            Expr::Dereference { expr, .. } => {
                // Dereference should be handled in HIR codegen
                todo!("Codegen for Dereference not implemented in AST codegen")
            }
            Expr::Index { .. } => {
                todo!("Codegen for Index not implemented in AST codegen")
            }
        }
    }

    /// Generate binary operation
    pub(super) fn generate_binary_op(
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
    pub(super) fn generate_unary_op(
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
            UnaryOp::Ref => {
                // Reference operator returns a pointer to the value
                // The value should already be a pointer from the alloca
                Ok(val)
            }
        }
    }

    /// Generate function call
    pub(super) fn generate_call(
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
    pub(super) fn generate_io_println(
        &mut self,
        args: &[Expr],
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
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
    pub(super) fn parse_format_string(
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
    pub(super) fn generate_if(
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
    pub(super) fn generate_expr_if(
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
    pub(super) fn coerce_type(
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
    pub(super) fn llvm_type(&self, ty: &Type) -> BasicTypeEnum<'ctx> {
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
                        // Slice: { *T, i64 }
                        let ptr_type = self.context.ptr_type(inkwell::AddressSpace::default());
                        let len_type = self.context.i64_type();
                        self.context
                            .struct_type(&[ptr_type.into(), len_type.into()], false)
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
    pub(super) fn llvm_type_to_lang(&self, ty: &BasicTypeEnum<'ctx>) -> Type {
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
    pub(super) fn coerce_to_llvm_type(
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

    /// Generate code for a type cast expression
    pub(super) fn generate_cast(
        &mut self,
        expr_value: BasicValueEnum<'ctx>,
        target_type: &Type,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        let target_llvm_type = self.llvm_type(target_type);
        self.coerce_to_llvm_type(expr_value, target_llvm_type)
    }

    /// Print the generated LLVM IR
    pub fn print_ir(&self) -> String {
        self.module.print_to_string().to_string()
    }
}
