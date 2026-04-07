//! # Symbol Command
//!
//! This module implements the `symbol` CLI command which lists all symbols
//! in a target project. It supports filtering by symbol kind (struct, interface,
//! function, enum, error, const) and searching by name.

use crate::ast::Visibility;
use crate::parser::parse;
use crate::sema::SemanticAnalyzer;
use crate::stdlib::StdLib;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SymbolKind {
    Struct,
    Interface,
    Function,
    Enum,
    Error,
    Const,
    All,
}

impl SymbolKind {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "struct" => Some(SymbolKind::Struct),
            "interface" => Some(SymbolKind::Interface),
            "fn" | "function" => Some(SymbolKind::Function),
            "enum" => Some(SymbolKind::Enum),
            "error" => Some(SymbolKind::Error),
            "const" => Some(SymbolKind::Const),
            "all" => Some(SymbolKind::All),
            _ => None,
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            SymbolKind::Struct => "struct",
            SymbolKind::Interface => "interface",
            SymbolKind::Function => "function",
            SymbolKind::Enum => "enum",
            SymbolKind::Error => "error",
            SymbolKind::Const => "const",
            SymbolKind::All => "all",
        }
    }
}

pub struct SymbolInfo {
    pub name: String,
    pub kind: SymbolKind,
    pub visibility: Visibility,
    pub type_info: String,
    pub location: String,
}

pub fn run_symbol_command(
    source: &str,
    _std_path: Option<std::path::PathBuf>,
    kinds: Vec<SymbolKind>,
    search_pattern: Option<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut program = parse(source)?;

    let mut stdlib = StdLib::new();
    let stdlib_path = crate::cmd::resolve_std_path(_std_path);
    stdlib.set_std_path(stdlib_path.to_str().unwrap());
    let _ = stdlib.preload_builtins();

    for (_, package_name) in &program.imports {
        let _ = stdlib.load_package(package_name);
    }

    let mut analyzer = SemanticAnalyzer::new();
    match analyzer.analyze_with_stdlib(&mut program, Some(&stdlib), true) {
        Ok(_) => {
            let symbols = collect_symbols(&analyzer, &program);
            let filtered_symbols = filter_symbols(symbols, &kinds, &search_pattern);
            print_symbols(filtered_symbols);
        }
        Err(e) => {
            eprintln!("Warning: Semantic analysis had errors: {}", e);
            let symbols = collect_symbols(&analyzer, &program);
            let filtered_symbols = filter_symbols(symbols, &kinds, &search_pattern);
            print_symbols(filtered_symbols);
        }
    }

    Ok(())
}

fn collect_symbols(analyzer: &SemanticAnalyzer, _program: &crate::ast::Program) -> Vec<SymbolInfo> {
    let mut symbols = Vec::new();

    for (name, struct_def) in &analyzer.structs {
        let type_info = format_struct_info(struct_def);
        symbols.push(SymbolInfo {
            name: name.clone(),
            kind: SymbolKind::Struct,
            visibility: struct_def.visibility,
            type_info,
            location: format!("struct definition"),
        });
    }

    for (name, interface_def) in &analyzer.interfaces {
        let type_info = format_interface_info(interface_def);
        symbols.push(SymbolInfo {
            name: name.clone(),
            kind: SymbolKind::Interface,
            visibility: interface_def.visibility,
            type_info,
            location: format!("interface definition"),
        });
    }

    for (name, enum_def) in &analyzer.enums {
        let type_info = format_enum_info(enum_def);
        symbols.push(SymbolInfo {
            name: name.clone(),
            kind: SymbolKind::Enum,
            visibility: enum_def.visibility,
            type_info,
            location: format!("enum definition"),
        });
    }

    for (name, error_def) in &analyzer.errors {
        let type_info = format_error_info(error_def);
        symbols.push(SymbolInfo {
            name: name.clone(),
            kind: SymbolKind::Error,
            visibility: error_def.visibility,
            type_info,
            location: format!("error definition"),
        });
    }

    for (name, fn_def) in &analyzer.functions {
        let type_info = format_fn_info(fn_def);
        symbols.push(SymbolInfo {
            name: name.clone(),
            kind: SymbolKind::Function,
            visibility: fn_def.visibility,
            type_info,
            location: format!("function definition"),
        });
    }

    let table = analyzer.get_symbol_table();
    for (name, symbol) in &table.scopes[0].symbols {
        if symbol.is_const {
            let ty = &symbol.ty;
            let is_function = matches!(ty, crate::ast::Type::Function { .. });
            let is_custom = matches!(ty, crate::ast::Type::Custom { .. });
            let is_error = matches!(ty, crate::ast::Type::Error);
            if is_function || is_custom || is_error {
                continue;
            }
            let type_info = format_type(&symbol.ty);
            symbols.push(SymbolInfo {
                name: name.clone(),
                kind: SymbolKind::Const,
                visibility: symbol.visibility,
                type_info,
                location: format!("constant"),
            });
        }
    }

    symbols
}

fn filter_symbols(
    mut symbols: Vec<SymbolInfo>,
    kinds: &[SymbolKind],
    search_pattern: &Option<String>,
) -> Vec<SymbolInfo> {
    if !kinds.is_empty() && !kinds.contains(&SymbolKind::All) {
        symbols.retain(|s| kinds.contains(&s.kind));
    }

    if let Some(pattern) = search_pattern {
        let pattern_lower = pattern.to_lowercase();
        symbols.retain(|s| s.name.to_lowercase().contains(&pattern_lower));
    }

    symbols.sort_by(|a, b| a.kind.cmp(&b.kind).then(a.name.cmp(&b.name)));
    symbols
}

fn print_symbols(symbols: Vec<SymbolInfo>) {
    println!("{:10} | {:12} | {}", "Kind", "Visibility", "Name (Type)");
    println!("------------|--------------|------------------------------------------------");

    for symbol in &symbols {
        let vis_str = match symbol.visibility {
            Visibility::Public => "public",
            Visibility::Private => "private",
        };
        println!(
            "{:10} | {:12} | {} {}",
            symbol.kind.display_name(),
            vis_str,
            symbol.name,
            symbol.type_info
        );
    }

    println!();
    println!("Total symbols: {}", symbols.len());
}

fn format_struct_info(struct_def: &crate::ast::StructDef) -> String {
    if struct_def.generic_params.is_empty() {
        String::new()
    } else {
        let params = struct_def
            .generic_params
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        format!("<{}>", params)
    }
}

fn format_interface_info(interface_def: &crate::ast::InterfaceDef) -> String {
    let methods: Vec<String> = interface_def
        .methods
        .iter()
        .map(|m| m.name.clone())
        .collect();
    if methods.is_empty() {
        String::new()
    } else {
        format!("{{ {} }}", methods.join(", "))
    }
}

fn format_enum_info(enum_def: &crate::ast::EnumDef) -> String {
    let variants: Vec<String> = enum_def.variants.iter().map(|v| v.name.clone()).collect();
    if variants.is_empty() {
        String::new()
    } else {
        format!("{{ {} }}", variants.join(", "))
    }
}

fn format_error_info(error_def: &crate::ast::ErrorDef) -> String {
    let variants: Vec<String> = error_def.variants.iter().map(|v| v.name.clone()).collect();
    if variants.is_empty() {
        String::new()
    } else {
        format!("{{ {} }}", variants.join(", "))
    }
}

fn format_fn_info(fn_def: &crate::ast::FnDef) -> String {
    let params: Vec<String> = fn_def.params.iter().map(|p| format_type(&p.ty)).collect();
    let params_str = params.join(", ");
    let return_str = format_type(&fn_def.return_ty);
    format!("({}) -> {}", params_str, return_str)
}

fn format_type(ty: &crate::ast::Type) -> String {
    match ty {
        crate::ast::Type::I8 => "i8".to_string(),
        crate::ast::Type::I16 => "i16".to_string(),
        crate::ast::Type::I32 => "i32".to_string(),
        crate::ast::Type::I64 => "i64".to_string(),
        crate::ast::Type::U8 => "u8".to_string(),
        crate::ast::Type::U16 => "u16".to_string(),
        crate::ast::Type::U32 => "u32".to_string(),
        crate::ast::Type::U64 => "u64".to_string(),
        crate::ast::Type::F32 => "f32".to_string(),
        crate::ast::Type::F64 => "f64".to_string(),
        crate::ast::Type::ImmInt => "ImmInt".to_string(),
        crate::ast::Type::ImmFloat => "ImmFloat".to_string(),
        crate::ast::Type::Bool => "bool".to_string(),
        crate::ast::Type::Void => "void".to_string(),
        crate::ast::Type::RawPtr => "rawptr".to_string(),
        crate::ast::Type::SelfType => "Self".to_string(),
        crate::ast::Type::Pointer(inner) => format!("*{}", format_type(inner)),
        crate::ast::Type::Option(inner) => format!("?{}", format_type(inner)),
        crate::ast::Type::Tuple(types) => {
            let inner = types
                .iter()
                .map(|t| format_type(t))
                .collect::<Vec<_>>()
                .join(", ");
            format!("({})", inner)
        }
        crate::ast::Type::Custom {
            name, generic_args, ..
        } => {
            if generic_args.is_empty() {
                name.clone()
            } else {
                let args = generic_args
                    .iter()
                    .map(|t| format_type(t))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("{}<{}>", name, args)
            }
        }
        crate::ast::Type::GenericParam(name) => name.clone(),
        crate::ast::Type::Array { size, element_type } => {
            if let Some(size) = size {
                format!("[{}]{}", size, format_type(element_type))
            } else {
                format!("[]{}", format_type(element_type))
            }
        }
        crate::ast::Type::Error => "error".to_string(),
        crate::ast::Type::Result(inner) => format!("{}!", format_type(inner)),
        crate::ast::Type::Const(inner) => format!("const {}", format_type(inner)),
        crate::ast::Type::Function {
            params,
            return_type,
        } => {
            let params_str = params
                .iter()
                .map(|t| format_type(t))
                .collect::<Vec<_>>()
                .join(", ");
            format!("fn({}) {}", params_str, format_type(return_type))
        }
        crate::ast::Type::VarArgs => "varargs".to_string(),
        crate::ast::Type::VarArgsPack(types) => {
            let inner = types
                .iter()
                .map(|t| format_type(t))
                .collect::<Vec<_>>()
                .join(", ");
            format!("varargs({})", inner)
        }
        crate::ast::Type::Package(name) => format!("package({})", name),
    }
}
