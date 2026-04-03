use super::*;
use crate::debug;

#[allow(unused)]
impl<'ctx> CodeGenerator<'ctx> {
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
            let method_name = if method.name.starts_with(&format!("{}_", struct_name)) {
                method.name.clone()
            } else {
                format!("{}_{}", struct_name, method.name)
            };
            let mangled_name = self.mangle_name(&method_name, false);
            let llvm_fn = self.module.add_function(&mangled_name, fn_type, None);
            self.add_varargs_param_attributes_to_function(llvm_fn, &param_types);
            if method.is_varargs_specialization {
                llvm_fn.set_linkage(inkwell::module::Linkage::LinkOnceODR);
                llvm_fn
                    .as_global_value()
                    .set_unnamed_address(UnnamedAddress::Global);
            }
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
        if debug::debug_enabled() {
            eprintln!(
                "DEBUG declare_function: name={}, return_ty={}",
                fn_def.name, fn_def.return_ty
            );
        }
        let fn_type =
            self.build_function_type(&fn_def.return_ty, &param_types, fn_def.name == "main");
        let mangled_name = self.mangle_name(&fn_def.name, fn_def.name == "main");

        let llvm_fn = self.module.add_function(&mangled_name, fn_type, None);
        self.add_varargs_param_attributes_to_function(llvm_fn, &param_types);
        if fn_def.is_varargs_specialization {
            llvm_fn.set_linkage(inkwell::module::Linkage::LinkOnceODR);
            llvm_fn
                .as_global_value()
                .set_unnamed_address(UnnamedAddress::Global);
        }

        Ok(())
    }

    /// Declare a function from a stdlib/legacy ast::FnDef (for internal use by process_imports)
    pub(super) fn declare_stdlib_function(&mut self, fn_def: &FnDef) -> CodegenResult<()> {
        let param_types: Vec<Type> = fn_def.params.iter().map(|p| p.ty.clone()).collect();
        let fn_type =
            self.build_function_type(&fn_def.return_ty, &param_types, fn_def.name == "main");
        let mangled_name = self.mangle_name(&fn_def.name, fn_def.name == "main");

        let llvm_fn = self.module.add_function(&mangled_name, fn_type, None);
        self.add_varargs_param_attributes_to_function(llvm_fn, &param_types);

        Ok(())
    }

    /// Declare a struct from a stdlib/legacy ast::StructDef (for internal use by process_imports)
    pub(super) fn declare_stdlib_struct(&mut self, struct_def: &StructDef) -> CodegenResult<()> {
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
            let method_name = if method.name.starts_with(&format!("{}_", struct_name)) {
                method.name.clone()
            } else {
                format!("{}_{}", struct_name, method.name)
            };
            let mangled_name = self.mangle_name(&method_name, false);
            let llvm_fn = self.module.add_function(&mangled_name, fn_type, None);
            self.add_varargs_param_attributes_to_function(llvm_fn, &param_types);
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
        if let Some(llvm_fn) = self.module.get_function(&mangled_name) {
            self.add_varargs_param_attributes_to_function(llvm_fn, &param_types);
        }

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
        if let Some(llvm_fn) = self.module.get_function(&ext_fn.name) {
            self.add_varargs_param_attributes_to_function(llvm_fn, &param_types);
        }

        Ok(())
    }

    /// Process imports and declare imported functions
    pub fn process_imports(&mut self, imports: &[(Option<String>, String)]) -> CodegenResult<()> {
        for (alias, package_name) in imports {
            let namespace = if let Some(a) = alias {
                a.clone()
            } else {
                // Extract the last part of the package name for the namespace
                // e.g., "utils/sub" -> "sub"
                package_name
                    .split('/')
                    .last()
                    .unwrap_or(package_name.as_str())
                    .to_string()
            };

            self.imported_packages
                .insert(namespace.to_string(), package_name.clone());

            // Get the last component of the package name as the module name for mangling
            let pkg_ns = package_name
                .split('/')
                .last()
                .unwrap_or(package_name.as_str());

            // If it's loaded in stdlib, declare its functions, structs, and enums
            if let Some(pkg) = self.stdlib.packages().get(package_name) {
                // Clone to avoid borrow issues
                let fn_defs = pkg.functions.clone();
                let ext_fns = pkg.external_functions.clone();
                let struct_defs = pkg.structs.clone();
                let enum_defs = pkg.enums.clone();

                // Declare regular functions
                for f in fn_defs {
                    if f.visibility == Visibility::Public {
                        self.declare_external_function(&f, pkg_ns)?;
                    }
                }
                // Declare external C functions (FFI)
                for ext_fn in ext_fns {
                    self.declare_c_function(&ext_fn)?;
                }
                // Declare structs
                for s in struct_defs {
                    if s.visibility == Visibility::Public {
                        let mut mangled_s = s.clone();
                        mangled_s.name = format!("{}_{}", pkg_ns, s.name);
                        self.declare_stdlib_struct(&mangled_s)?;
                    }
                }
                // Declare enums
                for e in enum_defs {
                    if e.visibility == Visibility::Public {
                        let mut mangled_e = e.clone();
                        mangled_e.name = format!("{}_{}", pkg_ns, e.name);
                        self.declare_enum(&mangled_e)?;
                    }
                }
            }
        }
        Ok(())
    }
}
