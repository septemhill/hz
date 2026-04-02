use crate::ast::{Expr, Span, Type};
use crate::sema::error::{AnalysisError, AnalysisResult};
use crate::sema::infer::{TypedExpr, TypedExprKind};

pub enum Intrinsic {
    IsNull,
    IsNotNull,
    TypeOf,
    SizeOf,
    AlignOf,
    BitCast,
}

impl Intrinsic {
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "@is_null" => Some(Intrinsic::IsNull),
            "@is_not_null" => Some(Intrinsic::IsNotNull),
            "@type_of" => Some(Intrinsic::TypeOf),
            "@size_of" => Some(Intrinsic::SizeOf),
            "@align_of" => Some(Intrinsic::AlignOf),
            "@bit_cast" => Some(Intrinsic::BitCast),
            _ => None,
        }
    }

    /// Basic validation of arguments (number of args, basic constraints)
    /// Used by SymbolResolver and TypeAnalyzer
    pub fn validate_args(&self, args: &[Expr], span: Span) -> AnalysisResult<()> {
        match self {
            Intrinsic::IsNull
            | Intrinsic::IsNotNull
            | Intrinsic::TypeOf
            | Intrinsic::SizeOf
            | Intrinsic::AlignOf => {
                if args.len() != 1 {
                    return Err(AnalysisError::new_with_span(
                        &format!("@{} requires exactly one argument", self.name()),
                        &span,
                    )
                    .with_module("intrinsics"));
                }
            }
            Intrinsic::BitCast => {
                if args.len() != 2 {
                    return Err(AnalysisError::new_with_span(
                        "@bit_cast requires exactly two arguments",
                        &span,
                    )
                    .with_module("intrinsics"));
                }
            }
        }

        // Specific checks for Expr-level arguments
        match self {
            Intrinsic::SizeOf | Intrinsic::AlignOf => {
                // Historically required a type literal, but let's be flexible if we want
                // Actually, the current infer.rs requires a type literal
                if !matches!(&args[0], Expr::TypeLiteral(_, _)) {
                    // We'll let this pass in resolver for now if we want to allow variables,
                    // but infer.rs might still complain.
                }
            }
            Intrinsic::BitCast => {
                if !matches!(&args[1], Expr::TypeLiteral(_, _)) {
                    return Err(AnalysisError::new_with_span(
                        "@bit_cast requires a type literal as its second argument",
                        &span,
                    )
                    .with_module("intrinsics"));
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// Detailed validation including types
    /// Used by TypeInferrer
    pub fn validate_types(&self, arg_types: &[Type], span: Span) -> AnalysisResult<()> {
        match self {
            Intrinsic::IsNull | Intrinsic::IsNotNull => {
                let ty = &arg_types[0];
                if !matches!(ty, Type::RawPtr | Type::Pointer(_) | Type::Option(_)) {
                    return Err(AnalysisError::new_with_span(
                        &format!(
                            "@{} requires a pointer, rawptr, or optional argument",
                            self.name()
                        ),
                        &span,
                    )
                    .with_module("intrinsics"));
                }
            }
            _ => {}
        }
        Ok(())
    }

    pub fn return_type_from_expr(&self, args: &[Expr]) -> Type {
        match self {
            Intrinsic::IsNull | Intrinsic::IsNotNull => Type::Bool,
            Intrinsic::TypeOf => Type::Array {
                size: None,
                element_type: Box::new(Type::Const(Box::new(Type::U8))),
            },
            Intrinsic::SizeOf | Intrinsic::AlignOf => Type::U64,
            Intrinsic::BitCast => {
                if let Some(Expr::TypeLiteral(ty, _)) = args.get(1) {
                    ty.clone()
                } else {
                    Type::Void
                }
            }
        }
    }

    pub fn return_type_from_args(&self, args: &[TypedExpr]) -> Type {
        match self {
            Intrinsic::IsNull | Intrinsic::IsNotNull => Type::Bool,
            Intrinsic::TypeOf => Type::Array {
                size: None,
                element_type: Box::new(Type::Const(Box::new(Type::U8))),
            },
            Intrinsic::SizeOf | Intrinsic::AlignOf => Type::U64,
            Intrinsic::BitCast => {
                if let Some(arg) = args.get(1) {
                    if let TypedExprKind::TypeLiteral(ty) = &arg.expr {
                        ty.clone()
                    } else {
                        Type::Void
                    }
                } else {
                    Type::Void
                }
            }
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Intrinsic::IsNull => "is_null",
            Intrinsic::IsNotNull => "is_not_null",
            Intrinsic::TypeOf => "type_of",
            Intrinsic::SizeOf => "size_of",
            Intrinsic::AlignOf => "align_of",
            Intrinsic::BitCast => "bit_cast",
        }
    }
}
