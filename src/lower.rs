use crate::ast;
use crate::hir;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::*;

    fn dummy_span() -> Span {
        Span { start: 0, end: 0 }
    }

    #[test]
    fn test_lower_int_literal() {
        let mut ctx = LoweringContext::new();
        let expr = Expr::Int(42, dummy_span());
        let hir_expr = ctx.lower_expr(&expr);

        match hir_expr {
            hir::HirExpr::Int(val, ty, _) => {
                assert_eq!(val, 42);
                assert_eq!(ty, Type::I64);
            }
            _ => panic!("Expected Int expression"),
        }
    }

    #[test]
    fn test_lower_bool_literal() {
        let mut ctx = LoweringContext::new();
        let expr = Expr::Bool(true, dummy_span());
        let hir_expr = ctx.lower_expr(&expr);

        match hir_expr {
            hir::HirExpr::Bool(val, ty, _) => {
                assert_eq!(val, true);
                assert_eq!(ty, Type::Bool);
            }
            _ => panic!("Expected Bool expression"),
        }
    }

    #[test]
    fn test_lower_string_literal() {
        let mut ctx = LoweringContext::new();
        let expr = Expr::String("hello".to_string(), dummy_span());
        let hir_expr = ctx.lower_expr(&expr);

        match hir_expr {
            hir::HirExpr::String(val, ty, _) => {
                assert_eq!(val, "hello");
                assert!(matches!(ty, Type::Custom { name, .. } if name == "String"));
            }
            _ => panic!("Expected String expression"),
        }
    }

    #[test]
    fn test_lower_char_literal() {
        let mut ctx = LoweringContext::new();
        let expr = Expr::Char('a', dummy_span());
        let hir_expr = ctx.lower_expr(&expr);

        match hir_expr {
            hir::HirExpr::Char(val, ty, _) => {
                assert_eq!(val, 'a');
                assert_eq!(ty, Type::I8);
            }
            _ => panic!("Expected Char expression"),
        }
    }

    #[test]
    fn test_lower_null_literal() {
        let mut ctx = LoweringContext::new();
        let expr = Expr::Null(dummy_span());
        let hir_expr = ctx.lower_expr(&expr);

        match hir_expr {
            hir::HirExpr::Null(ty, _) => {
                // Check that it's a Pointer type
                match ty {
                    Type::Pointer(_) => {}
                    _ => panic!("Expected Pointer type"),
                }
            }
            _ => panic!("Expected Null expression"),
        }
    }

    #[test]
    fn test_lower_tuple() {
        let mut ctx = LoweringContext::new();
        let expr = Expr::Tuple(
            vec![Expr::Int(1, dummy_span()), Expr::Int(2, dummy_span())],
            dummy_span(),
        );
        let hir_expr = ctx.lower_expr(&expr);

        match hir_expr {
            hir::HirExpr::Tuple { vals, ty, .. } => {
                assert_eq!(vals.len(), 2);
                assert!(matches!(ty, Type::Tuple(tys) if tys.len() == 2));
            }
            _ => panic!("Expected Tuple expression"),
        }
    }

    #[test]
    fn test_lower_array() {
        let mut ctx = LoweringContext::new();
        let expr = Expr::Array(
            vec![
                Expr::Int(1, dummy_span()),
                Expr::Int(2, dummy_span()),
                Expr::Int(3, dummy_span()),
            ],
            dummy_span(),
        );
        let hir_expr = ctx.lower_expr(&expr);

        match hir_expr {
            hir::HirExpr::Array { vals, ty, .. } => {
                assert_eq!(vals.len(), 3);
                assert!(matches!(ty, Type::Array { size: Some(3), .. }));
            }
            _ => panic!("Expected Array expression"),
        }
    }

    #[test]
    fn test_lower_ident() {
        let mut ctx = LoweringContext::new();
        let expr = Expr::Ident("my_var".to_string(), dummy_span());
        let hir_expr = ctx.lower_expr(&expr);

        match hir_expr {
            hir::HirExpr::Ident(name, ty, _) => {
                assert_eq!(name, "my_var");
                assert_eq!(ty, Type::I64); // Placeholder type
            }
            _ => panic!("Expected Ident expression"),
        }
    }

    #[test]
    fn test_lower_binary_expr() {
        let mut ctx = LoweringContext::new();
        let expr = Expr::Binary {
            op: BinaryOp::Add,
            left: Box::new(Expr::Int(1, dummy_span())),
            right: Box::new(Expr::Int(2, dummy_span())),
            span: dummy_span(),
        };
        let hir_expr = ctx.lower_expr(&expr);

        match hir_expr {
            hir::HirExpr::Binary {
                op,
                left,
                right,
                ty,
                ..
            } => {
                assert_eq!(op, BinaryOp::Add);
                assert!(matches!(*left, hir::HirExpr::Int(1, ..)));
                assert!(matches!(*right, hir::HirExpr::Int(2, ..)));
                assert_eq!(ty, Type::I64);
            }
            _ => panic!("Expected Binary expression"),
        }
    }

    #[test]
    fn test_lower_unary_expr() {
        let mut ctx = LoweringContext::new();
        let expr = Expr::Unary {
            op: UnaryOp::Neg,
            expr: Box::new(Expr::Int(5, dummy_span())),
            span: dummy_span(),
        };
        let hir_expr = ctx.lower_expr(&expr);

        match hir_expr {
            hir::HirExpr::Unary { op, expr, ty, .. } => {
                assert_eq!(op, UnaryOp::Neg);
                assert!(matches!(*expr, hir::HirExpr::Int(5, ..)));
                assert_eq!(ty, Type::I64);
            }
            _ => panic!("Expected Unary expression"),
        }
    }

    #[test]
    fn test_lower_call_expr() {
        let mut ctx = LoweringContext::new();
        let expr = Expr::Call {
            name: "println".to_string(),
            namespace: Some("io".to_string()),
            args: vec![Expr::String("test".to_string(), dummy_span())],
            span: dummy_span(),
        };
        let hir_expr = ctx.lower_expr(&expr);

        match hir_expr {
            hir::HirExpr::Call {
                name,
                namespace,
                args,
                return_ty,
                ..
            } => {
                assert_eq!(name, "println");
                assert_eq!(namespace, Some("io".to_string()));
                assert_eq!(args.len(), 1);
                assert_eq!(return_ty, Type::Void);
            }
            _ => panic!("Expected Call expression"),
        }
    }

    #[test]
    fn test_lower_if_expr() {
        let mut ctx = LoweringContext::new();
        let expr = Expr::If {
            condition: Box::new(Expr::Bool(true, dummy_span())),
            capture: None,
            then_branch: Box::new(Expr::Int(1, dummy_span())),
            else_branch: Box::new(Expr::Int(0, dummy_span())),
            span: dummy_span(),
        };
        let hir_expr = ctx.lower_expr(&expr);

        match hir_expr {
            hir::HirExpr::If {
                condition,
                capture,
                then_branch,
                else_branch,
                ty,
                ..
            } => {
                assert!(matches!(*condition, hir::HirExpr::Bool(true, ..)));
                assert!(matches!(*then_branch, hir::HirExpr::Int(1, ..)));
                assert!(matches!(*else_branch, hir::HirExpr::Int(0, ..)));
                assert_eq!(ty, Type::Void);
            }
            _ => panic!("Expected If expression"),
        }
    }

    #[test]
    fn test_lower_block_expr() {
        let mut ctx = LoweringContext::new();
        let expr = Expr::Block {
            stmts: vec![
                Stmt::Expr {
                    expr: Expr::Int(1, dummy_span()),
                    span: dummy_span(),
                },
                Stmt::Expr {
                    expr: Expr::Int(2, dummy_span()),
                    span: dummy_span(),
                },
            ],
            span: dummy_span(),
        };
        let hir_expr = ctx.lower_expr(&expr);

        match hir_expr {
            hir::HirExpr::Block { stmts, ty, .. } => {
                assert_eq!(stmts.len(), 2);
                assert_eq!(ty, Type::Void);
            }
            _ => panic!("Expected Block expression"),
        }
    }

    #[test]
    fn test_lower_member_access() {
        let mut ctx = LoweringContext::new();
        let expr = Expr::MemberAccess {
            object: Box::new(Expr::Ident("obj".to_string(), dummy_span())),
            member: "field".to_string(),
            span: dummy_span(),
        };
        let hir_expr = ctx.lower_expr(&expr);

        match hir_expr {
            hir::HirExpr::MemberAccess {
                object, member, ty, ..
            } => {
                assert!(matches!(*object, hir::HirExpr::Ident(n, ..) if n == "obj"));
                assert_eq!(member, "field");
                assert_eq!(ty, Type::I64); // Placeholder
            }
            _ => panic!("Expected MemberAccess expression"),
        }
    }

    #[test]
    fn test_lower_struct() {
        let mut ctx = LoweringContext::new();
        let expr = Expr::Struct {
            name: "Person".to_string(),
            fields: vec![
                (
                    "name".to_string(),
                    Expr::String("Alice".to_string(), dummy_span()),
                ),
                ("age".to_string(), Expr::Int(30, dummy_span())),
            ],
            span: dummy_span(),
        };
        let hir_expr = ctx.lower_expr(&expr);

        match hir_expr {
            hir::HirExpr::Struct {
                name, fields, ty, ..
            } => {
                assert_eq!(name, "Person");
                assert_eq!(fields.len(), 2);
                assert!(matches!(ty, Type::Custom { name: n, .. } if n == "Person"));
            }
            _ => panic!("Expected Struct expression"),
        }
    }

    #[test]
    fn test_lower_let_stmt() {
        let mut ctx = LoweringContext::new();
        let stmt = Stmt::Let {
            mutability: Mutability::Const,
            name: "x".to_string(),
            names: None,
            ty: Some(Type::I64),
            value: Some(Expr::Int(10, dummy_span())),
            visibility: Visibility::Private,
            span: dummy_span(),
        };
        let hir_stmt = ctx.lower_stmt(&stmt);

        match hir_stmt {
            hir::HirStmt::Let {
                name,
                ty,
                value,
                mutability,
                ..
            } => {
                assert_eq!(name, "x");
                assert_eq!(ty, Type::I64);
                assert_eq!(mutability, Mutability::Const);
                assert!(matches!(value, Some(hir::HirExpr::Int(10, ..))));
            }
            _ => panic!("Expected Let statement"),
        }
    }

    #[test]
    fn test_lower_return_stmt() {
        let mut ctx = LoweringContext::new();
        let stmt = Stmt::Return {
            value: Some(Expr::Int(42, dummy_span())),
            span: dummy_span(),
        };
        let hir_stmt = ctx.lower_stmt(&stmt);

        match hir_stmt {
            hir::HirStmt::Return(value, _) => {
                assert!(matches!(value, Some(hir::HirExpr::Int(42, ..))));
            }
            _ => panic!("Expected Return statement"),
        }
    }

    #[test]
    fn test_lower_if_stmt() {
        let mut ctx = LoweringContext::new();
        let stmt = Stmt::If {
            condition: Expr::Bool(true, dummy_span()),
            capture: None,
            then_branch: Box::new(Stmt::Expr {
                expr: Expr::Int(1, dummy_span()),
                span: dummy_span(),
            }),
            else_branch: Some(Box::new(Stmt::Expr {
                expr: Expr::Int(0, dummy_span()),
                span: dummy_span(),
            })),
            span: dummy_span(),
        };
        let hir_stmt = ctx.lower_stmt(&stmt);

        match hir_stmt {
            hir::HirStmt::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                assert!(matches!(condition, hir::HirExpr::Bool(true, ..)));
                assert!(matches!(
                    *then_branch,
                    hir::HirStmt::Expr(hir::HirExpr::Int(1, ..))
                ));
                assert!(else_branch.is_some());
            }
            _ => panic!("Expected If statement"),
        }
    }

    #[test]
    fn test_lower_program() {
        let mut ctx = LoweringContext::new();

        let program = Program {
            functions: vec![FnDef {
                name: "main".to_string(),
                visibility: Visibility::Public,
                params: vec![],
                return_ty: Type::I64,
                body: vec![Stmt::Return {
                    value: Some(Expr::Int(0, dummy_span())),
                    span: dummy_span(),
                }],
                span: dummy_span(),
            }],
            external_functions: vec![],
            structs: vec![],
            enums: vec![],
            errors: vec![],
            imports: vec![],
        };

        let hir_program = ctx.lower_program(&program);

        assert_eq!(hir_program.functions.len(), 1);
        assert_eq!(hir_program.functions[0].name, "main");
    }
}

pub struct LoweringContext {
    // Context for lowering (e.g. current scope, type info)
}

impl LoweringContext {
    pub fn new() -> Self {
        LoweringContext {}
    }

    pub fn lower_program(&mut self, program: &ast::Program) -> hir::HirProgram {
        hir::HirProgram {
            functions: program.functions.iter().map(|f| self.lower_fn(f)).collect(),
        }
    }

    fn lower_fn(&mut self, f: &ast::FnDef) -> hir::HirFn {
        hir::HirFn {
            name: f.name.clone(),
            params: f
                .params
                .iter()
                .map(|p| (p.name.clone(), p.ty.clone()))
                .collect(),
            return_ty: f.return_ty.clone(),
            body: f.body.iter().map(|s| self.lower_stmt(s)).collect(),
            visibility: f.visibility,
            span: f.span,
        }
    }

    /// Infer type from expression (simplified version)
    fn infer_type(&self, expr: &ast::Expr) -> Option<ast::Type> {
        match expr {
            ast::Expr::Ident(name, _) => {
                // For identifiers, we'd need scope lookup - return None for now
                None
            }
            ast::Expr::Call {
                name, namespace, ..
            } => {
                // For function calls, we can't easily infer the return type
                // without a symbol table - just return None
                let _ = (name, namespace); // suppress unused warnings
                None
            }
            ast::Expr::Try { expr, span } => {
                // For try expressions, try to get inner type
                self.infer_type(expr)
            }
            ast::Expr::Struct { name, .. } => Some(ast::Type::Custom {
                name: name.clone(),
                generic_args: vec![],
                is_exported: false,
            }),
            _ => None,
        }
    }

    fn lower_stmt(&mut self, s: &ast::Stmt) -> hir::HirStmt {
        match s {
            ast::Stmt::Expr { expr, .. } => hir::HirStmt::Expr(self.lower_expr(expr)),
            ast::Stmt::Let {
                name,
                ty,
                value,
                mutability,
                span,
                ..
            } => {
                // If no type annotation, we need to infer from the value
                // For now, require explicit type annotation
                let inferred_ty = value.as_ref().and_then(|v| self.infer_type(v));

                let ty = ty.clone().or(inferred_ty).unwrap_or(ast::Type::I64);

                hir::HirStmt::Let {
                    name: name.clone(),
                    ty,
                    value: value.as_ref().map(|v| self.lower_expr(v)),
                    mutability: *mutability,
                    span: *span,
                }
            }
            ast::Stmt::Return { value, span } => {
                hir::HirStmt::Return(value.as_ref().map(|v| self.lower_expr(v)), *span)
            }
            ast::Stmt::Block { stmts, span } => {
                // For now, simplify Stmt::Block into a nested sequence or stay as is
                // In a real HIR, we might flatten this
                hir::HirStmt::Expr(hir::HirExpr::Block {
                    stmts: stmts.iter().map(|s| self.lower_stmt(s)).collect(),
                    expr: None,
                    ty: ast::Type::Void,
                    span: *span,
                })
            }
            ast::Stmt::If {
                condition,
                capture,
                then_branch,
                else_branch,
                span,
                ..
            } => hir::HirStmt::If {
                condition: self.lower_expr(condition),
                capture: capture.clone(),
                then_branch: Box::new(self.lower_stmt(then_branch)),
                else_branch: else_branch.as_ref().map(|e| Box::new(self.lower_stmt(e))),
                span: *span,
            },
            ast::Stmt::Defer { stmt, span } => {
                // Lower the deferred statement
                hir::HirStmt::Defer {
                    stmt: Box::new(self.lower_stmt(stmt)),
                    span: *span,
                }
            }
            ast::Stmt::DeferBang { stmt, span } => {
                // Lower the deferred! statement (executes only on error)
                hir::HirStmt::DeferBang {
                    stmt: Box::new(self.lower_stmt(stmt)),
                    span: *span,
                }
            }
            ast::Stmt::Assign {
                target,
                value,
                span,
                ..
            } => {
                // For now, just lower as expression statement
                hir::HirStmt::Expr(hir::HirExpr::Block {
                    stmts: vec![],
                    expr: Some(Box::new(self.lower_expr(value))),
                    ty: ast::Type::Void,
                    span: *span,
                })
            }
            ast::Stmt::For {
                var_name,
                iterable,
                body,
                span,
                ..
            } => hir::HirStmt::For {
                var_name: var_name.clone(),
                iterable: self.lower_expr(iterable),
                body: Box::new(self.lower_stmt(body)),
                span: *span,
            },
            ast::Stmt::Switch {
                condition,
                cases,
                span,
                ..
            } => {
                // For now, lower as if-else chain
                hir::HirStmt::Switch {
                    condition: self.lower_expr(condition),
                    cases: cases
                        .iter()
                        .map(|c| hir::HirCase {
                            patterns: c.patterns.iter().map(|p| self.lower_expr(p)).collect(),
                            body: self.lower_stmt(&c.body),
                            span: c.span,
                        })
                        .collect(),
                    span: *span,
                }
            }
            ast::Stmt::Import { span, .. } => {
                // Import statements are handled at the module level, not in functions
                // Just return an empty expression for now
                hir::HirStmt::Expr(hir::HirExpr::Block {
                    stmts: vec![],
                    expr: None,
                    ty: ast::Type::Void,
                    span: *span,
                })
            }
        }
    }

    fn lower_expr(&mut self, e: &ast::Expr) -> hir::HirExpr {
        match e {
            ast::Expr::Int(v, span) => hir::HirExpr::Int(*v, ast::Type::I64, *span),
            ast::Expr::Bool(v, span) => hir::HirExpr::Bool(*v, ast::Type::Bool, *span),
            ast::Expr::String(v, span) => hir::HirExpr::String(
                v.clone(),
                ast::Type::Custom {
                    name: "String".to_string(),
                    generic_args: vec![],
                    is_exported: false,
                },
                *span,
            ),
            ast::Expr::Char(v, span) => hir::HirExpr::Char(*v, ast::Type::I8, *span),
            ast::Expr::Null(span) => {
                hir::HirExpr::Null(ast::Type::Pointer(Box::new(ast::Type::Void)), *span)
            }
            ast::Expr::Try { expr, span } => hir::HirExpr::Try {
                expr: Box::new(self.lower_expr(expr)),
                span: *span,
            },
            ast::Expr::Catch {
                expr,
                error_var,
                body,
                span,
            } => hir::HirExpr::Catch {
                expr: Box::new(self.lower_expr(expr)),
                error_var: error_var.clone(),
                body: Box::new(self.lower_expr(body)),
                span: *span,
            },
            ast::Expr::Tuple(vals, span) => hir::HirExpr::Tuple {
                vals: vals.iter().map(|v| self.lower_expr(v)).collect(),
                ty: ast::Type::Tuple(vals.iter().map(|_| ast::Type::I64).collect()), // Placeholder
                span: *span,
            },
            ast::Expr::TupleIndex { tuple, index, span } => hir::HirExpr::TupleIndex {
                tuple: Box::new(self.lower_expr(tuple)),
                index: *index,
                ty: ast::Type::I64, // Placeholder
                span: *span,
            },
            ast::Expr::Array(vals, span) => hir::HirExpr::Array {
                vals: vals.iter().map(|v| self.lower_expr(v)).collect(),
                ty: ast::Type::Array {
                    size: Some(vals.len()),
                    element_type: Box::new(ast::Type::I64),
                }, // Placeholder
                span: *span,
            },
            ast::Expr::Ident(name, span) => {
                hir::HirExpr::Ident(name.clone(), ast::Type::I64, *span)
            } // Placeholder type
            ast::Expr::Binary {
                op,
                left,
                right,
                span,
            } => {
                hir::HirExpr::Binary {
                    op: *op,
                    left: Box::new(self.lower_expr(left)),
                    right: Box::new(self.lower_expr(right)),
                    ty: ast::Type::I64, // Placeholder
                    span: *span,
                }
            }
            ast::Expr::Unary { op, expr, span } => {
                hir::HirExpr::Unary {
                    op: *op,
                    expr: Box::new(self.lower_expr(expr)),
                    ty: ast::Type::I64, // Placeholder
                    span: *span,
                }
            }
            ast::Expr::Call {
                name,
                namespace,
                args,
                span,
            } => {
                hir::HirExpr::Call {
                    name: name.clone(),
                    namespace: namespace.clone(),
                    args: args.iter().map(|a| self.lower_expr(a)).collect(),
                    return_ty: ast::Type::Void, // Placeholder
                    span: *span,
                }
            }
            ast::Expr::If {
                condition,
                capture,
                then_branch,
                else_branch,
                span,
            } => {
                hir::HirExpr::If {
                    condition: Box::new(self.lower_expr(condition)),
                    capture: capture.clone(),
                    then_branch: Box::new(self.lower_expr(then_branch)),
                    else_branch: Box::new(self.lower_expr(else_branch)),
                    ty: ast::Type::Void, // Placeholder
                    span: *span,
                }
            }
            ast::Expr::Block { stmts, span } => {
                hir::HirExpr::Block {
                    stmts: stmts.iter().map(|s| self.lower_stmt(s)).collect(),
                    expr: None,
                    ty: ast::Type::Void, // Placeholder
                    span: *span,
                }
            }
            ast::Expr::MemberAccess {
                object,
                member,
                span,
            } => {
                hir::HirExpr::MemberAccess {
                    object: Box::new(self.lower_expr(object)),
                    member: member.clone(),
                    ty: ast::Type::I64, // Placeholder
                    span: *span,
                }
            }
            ast::Expr::Struct { name, fields, span } => hir::HirExpr::Struct {
                name: name.clone(),
                fields: fields
                    .iter()
                    .map(|(n, v)| (n.clone(), self.lower_expr(v)))
                    .collect(),
                ty: ast::Type::Custom {
                    name: name.clone(),
                    generic_args: vec![],
                    is_exported: false,
                },
                span: *span,
            },
        }
    }
}
