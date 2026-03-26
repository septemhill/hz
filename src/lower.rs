use crate::ast;
use crate::hir;
use crate::sema::SymbolTable;
use crate::sema::infer::{
    TypedExpr, TypedExprKind, TypedFnDef, TypedProgram, TypedStmt, TypedStmtKind,
};

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
                assert!(
                    matches!(ty, Type::Array { element_type, .. } if matches!(*element_type, Type::U8))
                );
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
            None,
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
    fn test_lower_typed_array_uses_declared_element_type_for_literals() {
        let mut ctx = LoweringContext::new();
        let expr = Expr::Array(
            vec![
                Expr::Int(1, dummy_span()),
                Expr::Int(2, dummy_span()),
                Expr::Int(3, dummy_span()),
            ],
            Some(Type::U8),
            dummy_span(),
        );
        let hir_expr = ctx.lower_expr(&expr);

        match hir_expr {
            hir::HirExpr::Array { vals, ty, .. } => {
                assert_eq!(
                    ty,
                    Type::Array {
                        size: Some(3),
                        element_type: Box::new(Type::U8),
                    }
                );
                assert!(
                    vals.iter()
                        .all(|val| matches!(val, hir::HirExpr::Int(_, Type::U8, _)))
                );
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
            generic_args: vec![],
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
            hir::HirExpr::Block {
                stmts, expr, ty, ..
            } => {
                assert_eq!(stmts.len(), 1);
                assert!(expr.is_some());
                assert_eq!(ty, Type::I64);
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
            kind: MemberAccessKind::Unknown,
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
            generic_args: vec![],
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
    fn test_lower_for_loop_capture_uses_iterable_element_type() {
        let mut ctx = LoweringContext::new();
        let stmt = Stmt::For {
            label: None,
            var_name: None,
            iterable: Expr::Array(
                vec![Expr::Char('a', dummy_span()), Expr::Char('b', dummy_span())],
                Some(Type::U8),
                dummy_span(),
            ),
            capture: Some("value".to_string()),
            index_var: None,
            body: Box::new(Stmt::Block {
                stmts: vec![Stmt::Expr {
                    expr: Expr::Ident("value".to_string(), dummy_span()),
                    span: dummy_span(),
                }],
                span: dummy_span(),
            }),
            span: dummy_span(),
        };

        let hir_stmt = ctx.lower_stmt(&stmt);

        match hir_stmt {
            hir::HirStmt::For { body, .. } => match *body {
                hir::HirStmt::Expr(hir::HirExpr::Block { stmts, .. }) => match &stmts[0] {
                    hir::HirStmt::Expr(hir::HirExpr::Ident(name, ty, _)) => {
                        assert_eq!(name, "value");
                        assert_eq!(*ty, Type::U8);
                    }
                    _ => panic!("Expected loop body identifier"),
                },
                _ => panic!("Expected lowered loop body block"),
            },
            _ => panic!("Expected For statement"),
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
                generic_params: vec![],
                span: dummy_span(),
            }],
            external_functions: vec![],
            structs: vec![],
            enums: vec![],
            errors: vec![],
            imports: vec![],
        };

        let main_fn = crate::sema::infer::TypedFnDef {
            name: "main".to_string(),
            visibility: Visibility::Public,
            params: vec![],
            return_ty: Type::I64,
            body: vec![crate::sema::infer::TypedStmt {
                stmt: crate::sema::infer::TypedStmtKind::Return {
                    value: Some(crate::sema::infer::TypedExpr {
                        expr: crate::sema::infer::TypedExprKind::Int(0),
                        ty: Type::I64,
                        span: dummy_span(),
                    }),
                },
                span: dummy_span(),
            }],
            span: dummy_span(),
        };

        let typed_program = crate::sema::infer::TypedProgram {
            functions: vec![main_fn],
            external_functions: Vec::new(),
            structs: Vec::new(),
            enums: Vec::new(),
            errors: Vec::new(),
            imports: Vec::new(),
            ast_functions: Vec::new(),
            ast_structs: Vec::new(),
        };

        let hir_program = ctx.lower_program(&program, &typed_program);

        assert_eq!(hir_program.functions.len(), 1);
        assert_eq!(hir_program.functions[0].name, "main");
    }
}

pub struct LoweringContext {
    // Context for lowering (e.g. current scope, type info)
    symbol_table: SymbolTable,
    errors: std::collections::HashMap<String, ast::ErrorDef>,
    function_returns: std::collections::HashMap<String, ast::Type>,
    local_scopes: Vec<std::collections::HashMap<String, ast::Type>>,
    expected_type: Option<ast::Type>,
    structs: std::collections::HashMap<String, crate::sema::infer::TypedStructDef>,
}

impl LoweringContext {
    pub fn new() -> Self {
        LoweringContext {
            symbol_table: SymbolTable::new(),
            errors: std::collections::HashMap::new(),
            function_returns: std::collections::HashMap::new(),
            local_scopes: Vec::new(),
            expected_type: None,
            structs: std::collections::HashMap::new(),
        }
    }

    /// Set the symbol table (called from the analyzer after semantic analysis)
    pub fn set_symbol_table(&mut self, symbol_table: SymbolTable) {
        self.symbol_table = symbol_table;
    }

    fn enter_local_scope(&mut self) {
        self.local_scopes.push(std::collections::HashMap::new());
    }

    fn exit_local_scope(&mut self) {
        self.local_scopes.pop();
    }

    fn define_local(&mut self, name: String, ty: ast::Type) {
        if name.is_empty() {
            return;
        }

        if let Some(scope) = self.local_scopes.last_mut() {
            scope.insert(name, ty);
        }
    }

    fn resolve_local(&self, name: &str) -> Option<ast::Type> {
        for scope in self.local_scopes.iter().rev() {
            if let Some(ty) = scope.get(name) {
                return Some(ty.clone());
            }
        }

        None
    }

    fn with_expected_type<T>(
        &mut self,
        expected_type: Option<ast::Type>,
        f: impl FnOnce(&mut Self) -> T,
    ) -> T {
        let previous_expected_type = self.expected_type.clone();
        self.expected_type = expected_type;
        let result = f(self);
        self.expected_type = previous_expected_type;
        result
    }

    pub fn lower_program(
        &mut self,
        program: &ast::Program,
        typed_program: &crate::sema::infer::TypedProgram,
    ) -> hir::HirProgram {
        self.errors.clear();
        self.function_returns.clear();
        self.local_scopes.clear();
        self.expected_type = None;
        self.structs.clear();

        // Collect structs
        for s in &typed_program.structs {
            self.structs.insert(s.name.clone(), s.clone());
        }

        // Collect errors from the program
        for e in &program.errors {
            self.errors.insert(e.name.clone(), e.clone());
        }

        // Use typed_program functions (includes monomorphized ones)
        for f in &typed_program.functions {
            self.function_returns
                .insert(f.name.clone(), f.return_ty.clone());
        }

        // Use typed_program structs
        for s in &typed_program.structs {
            for m in &s.methods {
                self.function_returns
                    .insert(format!("{}_{}", s.name, m.name), m.return_ty.clone());
            }
        }

        // Use typed_program enums
        for e in &program.enums {
            for m in &e.methods {
                self.function_returns
                    .insert(format!("{}_{}", e.name, m.name), m.return_ty.clone());
            }
        }

        let mut functions: Vec<hir::HirFn> = Vec::new();
        let mut seen_names = std::collections::HashSet::new();

        for f in &typed_program.functions {
            if seen_names.insert(f.name.clone()) {
                functions.push(self.lower_typed_fn(f));
            }
        }

        // Lower struct methods and add them to the global function list
        for s in &typed_program.structs {
            for m in &s.methods {
                let name = format!("{}_{}", s.name, m.name);
                if seen_names.insert(name.clone()) {
                    let mut hir_fn = self.lower_typed_fn(m);
                    // Prefix method name with struct name to match codegen expectations
                    hir_fn.name = name;
                    functions.push(hir_fn);
                }
            }
        }

        // Lower enum methods and add them to the global function list
        for e in &program.enums {
            for m in &e.methods {
                let name = format!("{}_{}", e.name, m.name);
                if seen_names.insert(name.clone()) {
                    let mut hir_fn = self.lower_fn(m, None);
                    hir_fn.name = name;
                    functions.push(hir_fn);
                }
            }
        }

        hir::HirProgram { functions }
    }

    fn lower_typed_fn(&mut self, f: &crate::sema::infer::TypedFnDef) -> hir::HirFn {
        self.enter_local_scope();
        for param in &f.params {
            self.define_local(param.name.clone(), param.ty.clone());
        }

        let body = f.body.iter().map(|s| self.lower_typed_stmt(s)).collect();
        self.exit_local_scope();

        hir::HirFn {
            name: f.name.clone(),
            params: f
                .params
                .iter()
                .map(|p| (p.name.clone(), p.ty.clone()))
                .collect(),
            return_ty: f.return_ty.clone(),
            body,
            visibility: f.visibility,
            span: f.span,
        }
    }

    fn lower_typed_stmt(&mut self, s: &TypedStmt) -> hir::HirStmt {
        match &s.stmt {
            TypedStmtKind::Expr { expr } => hir::HirStmt::Expr(self.lower_typed_expr(expr)),
            TypedStmtKind::Let {
                name,
                ty,
                value,
                is_const,
                ..
            } => {
                let lowered_value = value.as_ref().map(|v| self.lower_typed_expr(v));
                self.define_local(name.clone(), ty.clone());

                hir::HirStmt::Let {
                    name: name.clone(),
                    ty: ty.clone(),
                    value: lowered_value,
                    mutability: if *is_const {
                        ast::Mutability::Const
                    } else {
                        ast::Mutability::Var
                    },
                    span: s.span,
                }
            }
            TypedStmtKind::Return { value } => {
                hir::HirStmt::Return(value.as_ref().map(|v| self.lower_typed_expr(v)), s.span)
            }
            TypedStmtKind::Block { stmts } => {
                self.enter_local_scope();
                let lowered_stmts = stmts.iter().map(|s| self.lower_typed_stmt(s)).collect();
                self.exit_local_scope();

                hir::HirStmt::Expr(hir::HirExpr::Block {
                    stmts: lowered_stmts,
                    expr: None,
                    ty: ast::Type::Void,
                    span: s.span,
                })
            }
            TypedStmtKind::If {
                condition,
                capture,
                then_branch,
                else_branch,
            } => {
                let lowered_condition = self.lower_typed_expr(condition);

                self.enter_local_scope();
                if let (Some(capture_name), ast::Type::Option(inner_ty)) =
                    (capture.as_ref(), &condition.ty)
                {
                    self.define_local(capture_name.clone(), (*inner_ty).as_ref().clone());
                }
                let lowered_then = Box::new(self.lower_typed_stmt(then_branch));
                self.exit_local_scope();

                let lowered_else = else_branch.as_ref().map(|e| {
                    self.enter_local_scope();
                    let lowered = Box::new(self.lower_typed_stmt(e));
                    self.exit_local_scope();
                    lowered
                });

                hir::HirStmt::If {
                    condition: lowered_condition,
                    capture: capture.clone(),
                    then_branch: lowered_then,
                    else_branch: lowered_else,
                    span: s.span,
                }
            }
            TypedStmtKind::For {
                var_name,
                capture,
                index_var,
                iterable,
                body,
                ..
            } => {
                self.enter_local_scope();
                if let Some(name) = var_name {
                    self.define_local(name.clone(), ast::Type::I64); // Simplified
                }
                if let Some(name) = capture {
                    self.define_local(name.clone(), ast::Type::I64);
                }
                if let Some(name) = index_var {
                    self.define_local(name.clone(), ast::Type::I64);
                }
                let lowered_body = Box::new(self.lower_typed_stmt(body));
                self.exit_local_scope();

                hir::HirStmt::For {
                    label: None,
                    var_name: capture.clone().or(var_name.clone()),
                    index_var: index_var.clone(),
                    iterable: self.lower_typed_expr(iterable),
                    body: lowered_body,
                    span: s.span,
                }
            }
            TypedStmtKind::Assign {
                target, value, op, ..
            } => {
                // Handle compound assignment: expand to target = target op value
                if *op != ast::AssignOp::Assign {
                    let target_ty = if target.contains('.') {
                        // For member access targets, we need to resolve the type
                        // This is a bit of a hack because Assign target is just a string.
                        // We'll try to resolve it as a member access.
                        let parts: Vec<&str> = target.split('.').collect();
                        let mut current_ty = self
                            .resolve_local(parts[0])
                            .or_else(|| self.symbol_table.resolve(parts[0]).map(|s| s.ty.clone()))
                            .unwrap_or(ast::Type::I64);

                        for i in 1..parts.len() {
                            let mut name_found = None;
                            let mut current = &current_ty;
                            while let ast::Type::Pointer(inner) | ast::Type::Option(inner) | ast::Type::Result(inner) = current {
                                current = inner.as_ref();
                            }
                            if let ast::Type::Custom { name, .. } = current {
                                name_found = Some(name);
                            }

                            if let Some(name) = name_found {
                                if let Some(s_def) = self.structs.get(name) {
                                    if let Some(field) =
                                        s_def.fields.iter().find(|f| f.name == parts[i])
                                    {
                                        current_ty = field.ty.clone();
                                        continue;
                                    }
                                }
                            }
                            // Fallback if we can't resolve
                            current_ty = ast::Type::I64;
                            break;
                        }
                        current_ty
                    } else {
                        self.resolve_local(target)
                            .or_else(|| self.symbol_table.resolve(target).map(|s| s.ty.clone()))
                            .unwrap_or(ast::Type::I64)
                    };

                    // Create read expression
                    let read_expr = if target.contains('.') {
                        let parts: Vec<&str> = target.split('.').collect();
                        let mut expr = hir::HirExpr::Ident(
                            parts[0].to_string(),
                            self.resolve_local(parts[0])
                                .or_else(|| {
                                    self.symbol_table.resolve(parts[0]).map(|s| s.ty.clone())
                                })
                                .unwrap_or(ast::Type::I64),
                            s.span,
                        );

                        let mut current_ty = expr.ty().clone();
                        for i in 1..parts.len() {
                            let mut name_found = None;
                            let mut current = &current_ty;
                            while let ast::Type::Pointer(inner) | ast::Type::Option(inner) | ast::Type::Result(inner) = current {
                                current = inner.as_ref();
                            }
                            if let ast::Type::Custom { name, .. } = current {
                                name_found = Some(name);
                            }

                            let member_ty = if let Some(name) = name_found {
                                if let Some(s_def) = self.structs.get(name) {
                                    s_def
                                        .fields
                                        .iter()
                                        .find(|f| f.name == parts[i])
                                        .map(|f| f.ty.clone())
                                        .unwrap_or(ast::Type::I64)
                                } else {
                                    ast::Type::I64
                                }
                            } else {
                                ast::Type::I64
                            };

                            expr = hir::HirExpr::MemberAccess {
                                object: Box::new(expr),
                                member: parts[i].to_string(),
                                ty: member_ty.clone(),
                                span: s.span,
                            };
                            current_ty = member_ty;
                        }
                        expr
                    } else {
                        hir::HirExpr::Ident(target.clone(), target_ty.clone(), s.span)
                    };

                    // Create binary operation
                    let bin_op = match op {
                        ast::AssignOp::AddAssign => ast::BinaryOp::Add,
                        ast::AssignOp::SubAssign => ast::BinaryOp::Sub,
                        ast::AssignOp::MulAssign => ast::BinaryOp::Mul,
                        ast::AssignOp::DivAssign => ast::BinaryOp::Div,
                        _ => ast::BinaryOp::Add,
                    };

                    let lowered_value = self.lower_typed_expr(value);

                    let result_expr = hir::HirExpr::Binary {
                        op: bin_op,
                        left: Box::new(read_expr),
                        right: Box::new(lowered_value),
                        ty: target_ty,
                        span: s.span,
                    };

                    return hir::HirStmt::Assign {
                        target: target.clone(),
                        value: result_expr,
                        span: s.span,
                    };
                }

                hir::HirStmt::Assign {
                    target: target.clone(),
                    value: self.lower_typed_expr(value),
                    span: s.span,
                }
            }
            TypedStmtKind::Break { .. } => hir::HirStmt::Break {
                label: None,
                span: s.span,
            },
            TypedStmtKind::Switch { condition, cases } => hir::HirStmt::Switch {
                condition: self.lower_typed_expr(condition),
                cases: cases
                    .iter()
                    .map(|c| hir::HirCase {
                        patterns: c
                            .patterns
                            .iter()
                            .map(|p| self.lower_typed_expr(p))
                            .collect(),
                        body: self.lower_typed_stmt(&c.body),
                        span: c.body.span,
                    })
                    .collect(),
                span: s.span,
            },
            TypedStmtKind::Defer { stmt } => hir::HirStmt::Defer {
                stmt: Box::new(self.lower_typed_stmt(stmt)),
                span: s.span,
            },
            TypedStmtKind::DeferBang { stmt } => hir::HirStmt::DeferBang {
                stmt: Box::new(self.lower_typed_stmt(stmt)),
                span: s.span,
            },
            _ => hir::HirStmt::Expr(hir::HirExpr::Null(ast::Type::Void, s.span)),
        }
    }

    fn lower_typed_expr(&mut self, e: &TypedExpr) -> hir::HirExpr {
        match &e.expr {
            TypedExprKind::Int(v) => hir::HirExpr::Int(*v, e.ty.clone(), e.span),
            TypedExprKind::Float(v) => hir::HirExpr::Float(*v, e.ty.clone(), e.span),
            TypedExprKind::Bool(v) => hir::HirExpr::Bool(*v, e.ty.clone(), e.span),
            TypedExprKind::String(v) => hir::HirExpr::String(v.clone(), e.ty.clone(), e.span),
            TypedExprKind::Char(v) => hir::HirExpr::Char(*v, e.ty.clone(), e.span),
            TypedExprKind::Null => hir::HirExpr::Null(e.ty.clone(), e.span),
            TypedExprKind::Ident(name) => hir::HirExpr::Ident(name.clone(), e.ty.clone(), e.span),
            TypedExprKind::Binary { op, left, right } => hir::HirExpr::Binary {
                op: *op,
                left: Box::new(self.lower_typed_expr(left)),
                right: Box::new(self.lower_typed_expr(right)),
                ty: e.ty.clone(),
                span: e.span,
            },
            TypedExprKind::Unary { op, expr } => hir::HirExpr::Unary {
                op: *op,
                expr: Box::new(self.lower_typed_expr(expr)),
                ty: e.ty.clone(),
                span: e.span,
            },
            TypedExprKind::Call {
                name,
                namespace,
                args,
            } => hir::HirExpr::Call {
                name: name.clone(),
                namespace: namespace.clone(),
                args: args.iter().map(|a| self.lower_typed_expr(a)).collect(),
                return_ty: e.ty.clone(),
                span: e.span,
            },
            TypedExprKind::If {
                condition,
                capture,
                then_branch,
                else_branch,
            } => hir::HirExpr::If {
                condition: Box::new(self.lower_typed_expr(condition)),
                capture: capture.clone(),
                then_branch: Box::new(self.lower_typed_expr(then_branch)),
                else_branch: Box::new(self.lower_typed_expr(else_branch)),
                ty: e.ty.clone(),
                span: e.span,
            },
            TypedExprKind::Block { stmts } => {
                self.enter_local_scope();
                let mut lowered_stmts = Vec::new();
                let mut last_expr = None;

                let len = stmts.len();
                for (i, s) in stmts.iter().enumerate() {
                    if i == len - 1 {
                        if let TypedStmtKind::Expr { expr } = &s.stmt {
                            last_expr = Some(Box::new(self.lower_typed_expr(expr)));
                            continue;
                        }
                    }
                    lowered_stmts.push(self.lower_typed_stmt(s));
                }

                self.exit_local_scope();
                hir::HirExpr::Block {
                    stmts: lowered_stmts,
                    expr: last_expr,
                    ty: e.ty.clone(),
                    span: e.span,
                }
            }
            TypedExprKind::Tuple(vals) => hir::HirExpr::Tuple {
                vals: vals.iter().map(|v| self.lower_typed_expr(v)).collect(),
                ty: e.ty.clone(),
                span: e.span,
            },
            TypedExprKind::TupleIndex { tuple, index } => hir::HirExpr::TupleIndex {
                tuple: Box::new(self.lower_typed_expr(tuple)),
                index: *index,
                ty: e.ty.clone(),
                span: e.span,
            },
            TypedExprKind::Array(vals) => hir::HirExpr::Array {
                vals: vals.iter().map(|v| self.lower_typed_expr(v)).collect(),
                ty: e.ty.clone(),
                span: e.span,
            },
            TypedExprKind::MemberAccess {
                object,
                member,
                kind: _,
            } => hir::HirExpr::MemberAccess {
                object: Box::new(self.lower_typed_expr(object)),
                member: member.clone(),
                ty: e.ty.clone(),
                span: e.span,
            },
            TypedExprKind::Struct { name, fields } => hir::HirExpr::Struct {
                name: name.clone(),
                fields: fields
                    .iter()
                    .map(|(n, v)| (n.clone(), self.lower_typed_expr(v)))
                    .collect(),
                ty: e.ty.clone(),
                span: e.span,
            },
            TypedExprKind::Try { expr } => hir::HirExpr::Try {
                expr: Box::new(self.lower_typed_expr(expr)),
                ty: e.ty.clone(),
                span: e.span,
            },
            TypedExprKind::Catch {
                expr,
                error_var,
                body,
            } => hir::HirExpr::Catch {
                expr: Box::new(self.lower_typed_expr(expr)),
                error_var: error_var.clone(),
                body: Box::new(self.lower_typed_expr(body)),
                ty: e.ty.clone(),
                span: e.span,
            },
        }
    }

    fn lower_fn(&mut self, f: &ast::FnDef, prefix: Option<&str>) -> hir::HirFn {
        let name = if let Some(p) = prefix {
            format!("{}_{}", p, f.name)
        } else {
            f.name.clone()
        };

        self.enter_local_scope();
        for param in &f.params {
            self.define_local(param.name.clone(), param.ty.clone());
        }

        let body = f.body.iter().map(|s| self.lower_stmt(s)).collect();
        self.exit_local_scope();

        hir::HirFn {
            name,
            params: f
                .params
                .iter()
                .map(|p| (p.name.clone(), p.ty.clone()))
                .collect(),
            return_ty: f.return_ty.clone(),
            body,
            visibility: f.visibility,
            span: f.span,
        }
    }

    /// Infer type from expression (simplified version)
    fn infer_type(&self, expr: &ast::Expr) -> Option<ast::Type> {
        match expr {
            ast::Expr::Ident(name, _) => {
                if let Some(ty) = self.resolve_local(name) {
                    return Some(ty);
                }

                // Look up identifier in symbol table
                if let Some(symbol) = self.symbol_table.resolve(name) {
                    Some(self.lang_type_to_ast_type(&symbol.ty))
                } else {
                    None
                }
            }
            ast::Expr::Call {
                name, namespace, ..
            } => {
                if let Some(ns) = namespace {
                    let receiver_ty = self
                        .resolve_local(ns)
                        .or_else(|| self.symbol_table.resolve(ns).map(|s| s.ty.clone()));
                    if let Some(struct_name) = receiver_ty
                        .as_ref()
                        .and_then(|ty| self.custom_type_name(ty))
                    {
                        if let Some(ty) = self
                            .function_returns
                            .get(&format!("{}_{}", struct_name, name))
                        {
                            return Some(ty.clone());
                        }
                    }

                    let qualified = format!("{}_{}", ns, name);
                    if let Some(ty) = self.function_returns.get(&qualified) {
                        return Some(ty.clone());
                    }
                } else if let Some(ty) = self.function_returns.get(name) {
                    return Some(ty.clone());
                }

                let resolved = if let Some(ns) = namespace {
                    let qualified = format!("{}::{}", ns, name);
                    self.symbol_table
                        .resolve(&qualified)
                        .or_else(|| self.symbol_table.resolve(name))
                } else {
                    self.symbol_table.resolve(name)
                };

                resolved.and_then(|symbol| match &symbol.ty {
                    ast::Type::Function { return_type, .. } => Some((**return_type).clone()),
                    ty => Some(ty.clone()),
                })
            }
            ast::Expr::Try { expr, span: _ } => match self.infer_type(expr) {
                Some(ast::Type::Result(inner)) => Some(*inner),
                Some(ty) => Some(ty),
                None => None,
            },
            ast::Expr::Tuple(elements, _) => Some(ast::Type::Tuple(
                elements
                    .iter()
                    .map(|element| self.infer_type(element).unwrap_or(ast::Type::I64))
                    .collect(),
            )),
            ast::Expr::TupleIndex { tuple, index, .. } => match self.infer_type(tuple) {
                Some(ast::Type::Tuple(types)) => types.get(*index).cloned(),
                _ => None,
            },
            ast::Expr::Struct { name, .. } => Some(ast::Type::Custom {
                name: name.clone(),
                generic_args: vec![],
                is_exported: false,
            }),
            ast::Expr::Array(elements, explicit_ty, _) => {
                // Infer array type from elements
                if let Some(ty) = explicit_ty {
                    // explicit_ty is the element type (e.g., u8), wrap it in Array
                    return Some(ast::Type::Array {
                        size: Some(elements.len()),
                        element_type: Box::new(ty.clone()),
                    });
                }
                // Infer from first element
                if let Some(first) = elements.first() {
                    if let Some(elem_ty) = self.infer_type(first) {
                        return Some(ast::Type::Array {
                            size: Some(elements.len()),
                            element_type: Box::new(elem_ty),
                        });
                    }
                }
                None
            }
            ast::Expr::Binary {
                op, left, right, ..
            } => match op {
                ast::BinaryOp::Range => {
                    let left_ty = self.infer_type(left)?;
                    let right_ty = self.infer_type(right)?;
                    Some(ast::Type::Tuple(vec![left_ty, right_ty]))
                }
                ast::BinaryOp::Eq
                | ast::BinaryOp::Ne
                | ast::BinaryOp::Lt
                | ast::BinaryOp::Gt
                | ast::BinaryOp::Le
                | ast::BinaryOp::Ge
                | ast::BinaryOp::And
                | ast::BinaryOp::Or => Some(ast::Type::Bool),
                _ => self.infer_type(left).or_else(|| self.infer_type(right)),
            },
            ast::Expr::Unary { expr, op, .. } => match op {
                ast::UnaryOp::Not => Some(ast::Type::Bool),
                _ => self.infer_type(expr),
            },
            ast::Expr::Char(_, _) => Some(ast::Type::I8),
            ast::Expr::Int(_, _) => Some(ast::Type::I64),
            ast::Expr::Float(_, _) => Some(ast::Type::F64),
            ast::Expr::Bool(_, _) => Some(ast::Type::Bool),
            _ => None,
        }
    }

    fn infer_for_binding_type(&self, iterable: &ast::Expr) -> Option<ast::Type> {
        if matches!(iterable, ast::Expr::Null(_)) {
            return None;
        }

        match self.infer_type(iterable)? {
            ast::Type::Array { element_type, .. } => Some(*element_type),
            ast::Type::Option(inner_ty) => Some(*inner_ty),
            ast::Type::Tuple(types) if !types.is_empty() => types.first().cloned(),
            _ => None,
        }
    }

    fn infer_for_index_type(&self, iterable: &ast::Expr) -> Option<ast::Type> {
        match self.infer_type(iterable)? {
            ast::Type::Array { .. } => Some(ast::Type::I64),
            _ => None,
        }
    }

    /// Convert from semantic analysis Type to AST Type (they're the same type)
    fn lang_type_to_ast_type(&self, lang_type: &ast::Type) -> ast::Type {
        lang_type.clone()
    }

    fn custom_type_name<'a>(&self, ty: &'a ast::Type) -> Option<&'a str> {
        match ty {
            ast::Type::Custom { name, .. } => Some(name.as_str()),
            ast::Type::Pointer(inner) => self.custom_type_name(inner),
            _ => None,
        }
    }

    fn lower_stmt(&mut self, s: &ast::Stmt) -> hir::HirStmt {
        match s {
            ast::Stmt::Expr { expr, .. } => hir::HirStmt::Expr(self.lower_expr(expr)),
            ast::Stmt::Let {
                name,
                names,
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
                let lowered_value = value
                    .as_ref()
                    .map(|v| self.with_expected_type(Some(ty.clone()), |ctx| ctx.lower_expr(v)));

                if let Some(bindings) = names {
                    if let ast::Type::Tuple(element_tys) = &ty {
                        for (index, binding) in bindings.iter().enumerate() {
                            if let Some(binding_name) = binding {
                                let binding_ty =
                                    element_tys.get(index).cloned().unwrap_or(ast::Type::I64);
                                self.define_local(binding_name.clone(), binding_ty);
                            }
                        }
                    }
                } else {
                    self.define_local(name.clone(), ty.clone());
                }

                hir::HirStmt::Let {
                    name: name.clone(),
                    ty,
                    value: lowered_value,
                    mutability: *mutability,
                    span: *span,
                }
            }
            ast::Stmt::Return { value, span } => {
                hir::HirStmt::Return(value.as_ref().map(|v| self.lower_expr(v)), *span)
            }
            ast::Stmt::Block { stmts, span } => {
                self.enter_local_scope();
                let lowered_stmts = stmts.iter().map(|s| self.lower_stmt(s)).collect();
                self.exit_local_scope();

                // For now, simplify Stmt::Block into a nested sequence or stay as is
                // In a real HIR, we might flatten this
                hir::HirStmt::Expr(hir::HirExpr::Block {
                    stmts: lowered_stmts,
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
            } => {
                let condition_ty = self.infer_type(condition);
                let lowered_condition = self.lower_expr(condition);

                self.enter_local_scope();
                if let (Some(capture_name), Some(ast::Type::Option(inner_ty))) =
                    (capture.as_ref(), condition_ty.clone())
                {
                    self.define_local(capture_name.clone(), *inner_ty);
                }
                let lowered_then = Box::new(self.lower_stmt(then_branch));
                self.exit_local_scope();

                let lowered_else = else_branch.as_ref().map(|e| {
                    self.enter_local_scope();
                    let lowered = Box::new(self.lower_stmt(e));
                    self.exit_local_scope();
                    lowered
                });

                hir::HirStmt::If {
                    condition: lowered_condition,
                    capture: capture.clone(),
                    then_branch: lowered_then,
                    else_branch: lowered_else,
                    span: *span,
                }
            }
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
                op,
                value,
                span,
            } => {
                eprintln!(
                    "DEBUG: Assign op={:?}, target={}, value={:?}",
                    op, target, value
                );
                // Handle compound assignment (e.g., c.age += 43)
                if *op != ast::AssignOp::Assign {
                    // For compound assignment, expand to: target = target op value
                    // Parse the target to get object and member
                    let (obj, member) = if target.contains('.') {
                        let parts: Vec<&str> = target.split('.').collect();
                        (parts[0].to_string(), Some(parts[1].to_string()))
                    } else {
                        (target.clone(), None)
                    };

                    // Infer the type from the target variable, not from the value expression
                    // For compound assignment like x += 1, we need the type of x, not 1
                    let target_ty = if target.contains('.') {
                        // For member access, we need to get the field type
                        self.infer_type(value).unwrap_or(ast::Type::I64)
                    } else {
                        // For simple variable, resolve from local scope or symbol table
                        let ty = self
                            .resolve_local(target)
                            .or_else(|| self.symbol_table.resolve(target).map(|s| s.ty.clone()))
                            .unwrap_or_else(|| self.infer_type(value).unwrap_or(ast::Type::I64));
                        eprintln!(
                            "DEBUG: compound assign target={}, target_ty={:?}",
                            target, ty
                        );
                        ty
                    };

                    // Create the read expression (read current value from target)
                    let read_expr = if let Some(member) = member {
                        hir::HirExpr::MemberAccess {
                            object: Box::new(hir::HirExpr::Ident(obj, target_ty.clone(), *span)),
                            member,
                            ty: target_ty.clone(),
                            span: *span,
                        }
                    } else {
                        hir::HirExpr::Ident(target.clone(), target_ty.clone(), *span)
                    };

                    // Create the binary operation
                    let bin_op = match op {
                        ast::AssignOp::AddAssign => ast::BinaryOp::Add,
                        ast::AssignOp::SubAssign => ast::BinaryOp::Sub,
                        ast::AssignOp::MulAssign => ast::BinaryOp::Mul,
                        ast::AssignOp::DivAssign => ast::BinaryOp::Div,
                        ast::AssignOp::Assign => ast::BinaryOp::Add,
                    };

                    // Lower the value expression
                    let lowered_value = self.lower_expr(value);

                    let result_expr = hir::HirExpr::Binary {
                        op: bin_op,
                        left: Box::new(read_expr),
                        right: Box::new(lowered_value),
                        ty: target_ty,
                        span: *span,
                    };

                    hir::HirStmt::Assign {
                        target: target.clone(),
                        value: result_expr,
                        span: *span,
                    }
                } else {
                    let target_expected_ty = if target.contains('.') {
                        None
                    } else {
                        self.resolve_local(target)
                            .or_else(|| self.symbol_table.resolve(target).map(|s| s.ty.clone()))
                    };

                    hir::HirStmt::Assign {
                        target: target.clone(),
                        value: self
                            .with_expected_type(target_expected_ty, |ctx| ctx.lower_expr(value)),
                        span: *span,
                    }
                }
            }
            ast::Stmt::For {
                label,
                var_name,
                capture,
                index_var,
                iterable,
                body,
                span,
                ..
            } => {
                let loop_binding_ty = self.infer_for_binding_type(iterable);
                let loop_index_ty = self.infer_for_index_type(iterable);

                self.enter_local_scope();
                if let Some(name) = var_name {
                    if let Some(ty) = loop_binding_ty.clone() {
                        self.define_local(name.clone(), ty);
                    }
                }
                if let Some(name) = capture {
                    if let Some(ty) = loop_binding_ty.clone() {
                        self.define_local(name.clone(), ty);
                    }
                }
                if let Some(name) = index_var {
                    if let Some(ty) = loop_index_ty.clone() {
                        self.define_local(name.clone(), ty);
                    }
                }
                let lowered_body = Box::new(self.lower_stmt(body));
                self.exit_local_scope();

                hir::HirStmt::For {
                    label: label.clone(),
                    // Use capture as the var_name (the loop variable)
                    var_name: capture.clone().or(var_name.clone()),
                    index_var: index_var.clone(),
                    iterable: self.lower_expr(iterable),
                    body: lowered_body,
                    span: *span,
                }
            }
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
            ast::Stmt::Break { label, span } => hir::HirStmt::Break {
                label: label.clone(),
                span: *span,
            },
        }
    }

    fn lower_expr(&mut self, e: &ast::Expr) -> hir::HirExpr {
        match e {
            ast::Expr::Int(v, span) => {
                let ty = self
                    .expected_type
                    .as_ref()
                    .filter(|ty| ty.is_integer())
                    .cloned()
                    .unwrap_or(ast::Type::I64);
                hir::HirExpr::Int(*v, ty, *span)
            }
            ast::Expr::Float(v, span) => hir::HirExpr::Float(*v, ast::Type::F64, *span),
            ast::Expr::Bool(v, span) => hir::HirExpr::Bool(*v, ast::Type::Bool, *span),
            ast::Expr::String(v, span) => hir::HirExpr::String(
                v.clone(),
                ast::Type::Array {
                    size: None,
                    element_type: Box::new(ast::Type::U8),
                },
                *span,
            ),
            ast::Expr::Char(v, span) => {
                let ty = self
                    .expected_type
                    .as_ref()
                    .filter(|ty| ty.is_integer())
                    .cloned()
                    .unwrap_or(ast::Type::I8);
                hir::HirExpr::Char(*v, ty, *span)
            }
            ast::Expr::Null(span) => {
                hir::HirExpr::Null(ast::Type::Pointer(Box::new(ast::Type::Void)), *span)
            }
            ast::Expr::Try { expr, span } => {
                let ty = self.infer_type(expr).and_then(|t| t.result_inner().map(|i| i.clone())).unwrap_or(ast::Type::Void);
                hir::HirExpr::Try {
                    expr: Box::new(self.lower_expr(expr)),
                    ty,
                    span: *span,
                }
            },
            ast::Expr::Catch {
                expr,
                error_var,
                body,
                span,
            } => {
                let ty = self.infer_type(expr).and_then(|t| t.result_inner().map(|i| i.clone())).unwrap_or(ast::Type::Void);
                hir::HirExpr::Catch {
                    expr: Box::new(self.lower_expr(expr)),
                    error_var: error_var.clone(),
                    body: Box::new(self.lower_expr(body)),
                    ty,
                    span: *span,
                }
            },
            ast::Expr::Tuple(vals, span) => hir::HirExpr::Tuple {
                vals: vals.iter().map(|v| self.lower_expr(v)).collect(),
                ty: self.infer_type(e).unwrap_or_else(|| {
                    ast::Type::Tuple(vals.iter().map(|_| ast::Type::I64).collect())
                }),
                span: *span,
            },
            ast::Expr::TupleIndex { tuple, index, span } => hir::HirExpr::TupleIndex {
                tuple: Box::new(self.lower_expr(tuple)),
                index: *index,
                ty: self.infer_type(e).unwrap_or(ast::Type::I64),
                span: *span,
            },
            ast::Expr::Array(vals, explicit_ty, span) => {
                let context_element_type = match self.expected_type.as_ref() {
                    Some(ast::Type::Array { element_type, .. }) => {
                        Some(element_type.as_ref().clone())
                    }
                    _ => None,
                };

                // Use explicit type if provided, otherwise infer from context or first element.
                let element_type = if let Some(ty) = explicit_ty {
                    ty.clone()
                } else if let Some(ty) = context_element_type {
                    ty
                } else if let Some(first) = vals.first() {
                    self.infer_type(first).unwrap_or(ast::Type::I64)
                } else {
                    ast::Type::I64
                };
                hir::HirExpr::Array {
                    vals: vals
                        .iter()
                        .map(|v| {
                            self.with_expected_type(Some(element_type.clone()), |ctx| {
                                ctx.lower_expr(v)
                            })
                        })
                        .collect(),
                    ty: ast::Type::Array {
                        size: Some(vals.len()),
                        element_type: Box::new(element_type),
                    },
                    span: *span,
                }
            }
            ast::Expr::Ident(name, span) => {
                // Look up the type from the current local scopes first, then the symbol table.
                let ty = self
                    .resolve_local(name)
                    .or_else(|| self.symbol_table.resolve(name).map(|s| s.ty.clone()))
                    .unwrap_or(ast::Type::I64); // Default to i64 if not found

                // If the resolved type is a primitive type (not a function),
                // check if this identifier is actually a function and convert to function type
                // This handles cases like: var a: fn(i64, i64) i64 = add;
                if !matches!(ty, ast::Type::Function { .. }) {
                    // Try to find a function definition with this name
                    // We need to look in the program's functions list
                    // For now, if the name exists in the symbol table with a return type,
                    // we'll need to handle this differently
                    // Actually, let's skip this for now - the codegen will handle it
                }

                hir::HirExpr::Ident(name.clone(), ty, *span)
            }
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
                generic_args: _,
                span,
            } => hir::HirExpr::Call {
                name: name.clone(),
                namespace: namespace.clone(),
                args: args.iter().map(|a| self.lower_expr(a)).collect(),
                return_ty: self.infer_type(e).unwrap_or(ast::Type::Void),
                span: *span,
            },
            ast::Expr::If {
                condition,
                capture,
                then_branch,
                else_branch,
                span,
            } => {
                let condition_ty = self.infer_type(condition);
                let lowered_condition = self.lower_expr(condition);

                self.enter_local_scope();
                if let (Some(capture_name), Some(ast::Type::Option(inner_ty))) =
                    (capture.as_ref(), condition_ty.clone())
                {
                    self.define_local(capture_name.clone(), *inner_ty);
                }
                let lowered_then = Box::new(self.lower_expr(then_branch));
                self.exit_local_scope();

                self.enter_local_scope();
                let lowered_else = Box::new(self.lower_expr(else_branch));
                self.exit_local_scope();

                hir::HirExpr::If {
                    condition: Box::new(lowered_condition),
                    capture: capture.clone(),
                    then_branch: lowered_then,
                    else_branch: lowered_else,
                    ty: ast::Type::Void, // Placeholder
                    span: *span,
                }
            }
            ast::Expr::Block { stmts, span } => {
                self.enter_local_scope();
                let mut lowered_stmts = Vec::new();
                let mut block_expr = None;
                let mut block_ty = ast::Type::Void;

                for (i, s) in stmts.iter().enumerate() {
                    if i == stmts.len() - 1 {
                        if let ast::Stmt::Expr { expr, .. } = s {
                            block_ty = self.infer_type(expr).unwrap_or(ast::Type::Void);
                            let lowered_e = self
                                .with_expected_type(self.expected_type.clone(), |ctx| {
                                    ctx.lower_expr(expr)
                                });
                            block_expr = Some(Box::new(lowered_e));
                            continue;
                        }
                    }
                    lowered_stmts.push(self.lower_stmt(s));
                }
                self.exit_local_scope();

                hir::HirExpr::Block {
                    stmts: lowered_stmts,
                    expr: block_expr,
                    ty: block_ty,
                    span: *span,
                }
            }
            ast::Expr::MemberAccess {
                object,
                member,
                kind: _,
                span,
            } => {
                // Try to resolve the type from the symbol table or check if it's an error member access
                let ty = if let ast::Expr::Ident(obj_name, _) = object.as_ref() {
                    // First check if this is an error type member access
                    if let Some(error_def) = self.errors.get(obj_name) {
                        if error_def.variants.iter().any(|v| &v.name == member) {
                            // This is an error member access (e.g., SampleError.CodegenError)
                            return hir::HirExpr::MemberAccess {
                                object: Box::new(self.lower_expr(object)),
                                member: member.clone(),
                                ty: ast::Type::Error,
                                span: *span,
                            };
                        }
                    }

                    // Then try to resolve the type from the symbol table
                    let full_name = format!("{}.{}", obj_name, member);
                    self.symbol_table
                        .resolve(&full_name)
                        .map(|s| s.ty.clone())
                        .unwrap_or(ast::Type::I64)
                } else {
                    ast::Type::I64 // Fallback for non-identifier objects
                };

                hir::HirExpr::MemberAccess {
                    object: Box::new(self.lower_expr(object)),
                    member: member.clone(),
                    ty,
                    span: *span,
                }
            }
            ast::Expr::Struct {
                name,
                fields,
                generic_args: _,
                span,
            } => hir::HirExpr::Struct {
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
