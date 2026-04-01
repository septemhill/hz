use super::*;

#[allow(unused)]
impl<'ctx> CodeGenerator<'ctx> {
    pub(crate) fn generate_hir_io_println(
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

                // Get string constants for boolean output
                let true_str = unsafe { self.builder.build_global_string("true", "true_str")? };
                let false_str = unsafe { self.builder.build_global_string("false", "false_str")? };

                for &(idx, kind) in &arg_specs {
                    if idx + 1 < args.len() {
                        let raw_val = self.generate_hir_expr(&args[idx + 1])?;

                        // Special handling for Boolean - convert to string pointer
                        if matches!(kind, PrintfArgKind::Boolean) {
                            // Convert to i1 first
                            let bool_val = if raw_val.is_int_value() {
                                let int_val = raw_val.into_int_value();
                                if int_val.get_type().get_bit_width() == 1 {
                                    int_val
                                } else {
                                    self.builder.build_int_cast(
                                        int_val,
                                        self.context.bool_type(),
                                        "to_bool",
                                    )?
                                }
                            } else if raw_val.is_float_value() {
                                // For floats, compare not equal to 0.0
                                let float_val = raw_val.into_float_value();
                                let zero = self.context.f64_type().const_float(0.0);
                                let cmp = self.builder.build_float_compare(
                                    inkwell::FloatPredicate::ONE,
                                    float_val,
                                    zero,
                                    "float_to_bool",
                                )?;
                                // Convert i1 to i64
                                self.builder.build_int_z_extend(
                                    cmp,
                                    self.context.i64_type(),
                                    "bool_ext",
                                )?
                            } else {
                                // For pointers, compare with null
                                let ptr = raw_val.into_pointer_value();
                                let null_ptr = self
                                    .context
                                    .ptr_type(inkwell::AddressSpace::default())
                                    .const_null();
                                self.builder.build_int_compare(
                                    inkwell::IntPredicate::NE,
                                    ptr,
                                    null_ptr,
                                    "ptr_to_bool",
                                )?
                            };

                            // Select between true_str and false_str
                            let selected_str = self
                                .builder
                                .build_select(bool_val, true_str, false_str, "bool_str")?;
                            llvm_args.push(selected_str.into());
                        } else {
                            let val = self.promote_printf_arg(raw_val, kind)?;
                            llvm_args.push(val.into());
                        }
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
                    BasicValueEnum::StructValue(sv) => {
                        // Check if it's a slice { ptr, len }
                        if sv.get_type().get_field_types().len() == 2
                            && sv.get_type().get_field_types()[0].is_pointer_type()
                            && sv.get_type().get_field_types()[1].is_int_type()
                        {
                            let ptr = self.builder.build_extract_value(sv, 0, "slice_ptr")?;
                            ("%s\n", ptr)
                        } else {
                            ("%lld\n", self.promote_printf_arg(raw_arg, PrintfArgKind::Integer)?)
                        }
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
                    "X" => {
                        // Uppercase Hex
                        result.push_str("%llX");
                        if arg_index < num_args {
                            arg_specs.push((arg_index, PrintfArgKind::Integer));
                            arg_index += 1;
                        }
                    }
                    "b" => {
                        // Boolean
                        result.push_str("%s");
                        if arg_index < num_args {
                            arg_specs.push((arg_index, PrintfArgKind::Boolean));
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
}
