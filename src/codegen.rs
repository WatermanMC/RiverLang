// last updated: june 2, 2026
// Code gen: converts RiverLang AST to C++ code
use crate::ast::*;
use crate::symtable::ClassInfo;
use std::collections::{HashMap, HashSet};
use std::io::Write;

const ANSI_RED: &str = "\x1b[31m";
const ANSI_RESET: &str = "\x1b[0m";

pub struct CodeGen<'a> {
    out: &'a mut dyn Write,
    _class_registry: &'a [ClassInfo], // not used in this version, but kept for compatibility
    current_class: String,
    current_method_vars: Vec<(String, usize)>, // variables in current method (name, dummy)
    local_var_types: HashMap<String, H20Type>,
    released_vars: HashSet<String>,
    zombie_vars: HashSet<String>,
    in_unsafe: bool,
}

impl<'a> CodeGen<'a> {
    pub fn new(out: &'a mut dyn Write, _class_registry: &'a [ClassInfo]) -> Self {
        Self {
            out,
            _class_registry,
            current_class: String::new(),
            current_method_vars: Vec::new(),
            local_var_types: HashMap::new(),
            released_vars: HashSet::new(),
            zombie_vars: HashSet::new(),
            in_unsafe: false,
        }
    }

    fn error(&self, msg: &str) -> ! {
        eprintln!("{}error: {}{}", ANSI_RED, msg, ANSI_RESET);
        std::process::exit(1);
    }

    fn emitln(&mut self, s: &str) {
        let _ = writeln!(self.out, "{}", s);
    }

    // Convert RiverLang type to C++ type string
    fn cpp_type(&self, typ: &H20Type, is_mut: bool) -> String {
        match typ {
            H20Type::Int => if is_mut { "long long".to_string() } else { "const long long".to_string() },
            H20Type::Float => if is_mut { "double".to_string() } else { "const double".to_string() },
            H20Type::Boolean => if is_mut { "bool".to_string() } else { "const bool".to_string() },
            H20Type::StringType => if is_mut { "std::string".to_string() } else { "const std::string".to_string() },
            H20Type::Void => "void".to_string(),
            H20Type::List(inner) => {
                let inner_cpp = self.cpp_type_bare(inner);
                format!("std::shared_ptr<std::vector<{}>>", inner_cpp)
            }
            H20Type::Custom(name) => name.clone(),
        }
    }

    fn cpp_type_bare(&self, typ: &H20Type) -> String {
        match typ {
            H20Type::Int => "long long".to_string(),
            H20Type::Float => "double".to_string(),
            H20Type::Boolean => "bool".to_string(),
            H20Type::StringType => "std::string".to_string(),
            H20Type::Void => "void".to_string(),
            H20Type::List(inner) => format!("std::shared_ptr<std::vector<{}>>", self.cpp_type_bare(inner)),
            H20Type::Custom(name) => name.clone(),
        }
    }

    fn zero_value(&self, typ: &H20Type) -> Option<String> {
        match typ {
            H20Type::Int => Some("0".to_string()),
            H20Type::Float => Some("0.0".to_string()),
            H20Type::Boolean => Some("false".to_string()),
            H20Type::StringType => Some("\"\"".to_string()),
            H20Type::List(_) => Some("nullptr".to_string()),
            _ => None,
        }
    }

    fn cpp_param_type(&self, typ: &H20Type, is_mut: bool) -> String {
        let is_primitive = matches!(typ, H20Type::Int | H20Type::Float | H20Type::Boolean);
        if is_mut {
            self.cpp_type_bare(typ)
        } else if is_primitive {
            format!("const {}", self.cpp_type_bare(typ))
        } else {
            format!("const {}&", self.cpp_type_bare(typ))
        }
    }

    pub fn gen_program(&mut self, program: &Program) {
        for item in &program.items {
            self.gen_top_level(item);
        }
    }

    fn gen_top_level(&mut self, item: &TopLevel) {
        match item {
            TopLevel::Import(_) => {}
            TopLevel::Include(s) => self.emitln(s),
            TopLevel::RawCpp(s) => self.emitln(s),
            TopLevel::Class(class) => self.gen_class(class),
        }
    }

    fn gen_class(&mut self, class: &ClassDecl) {
        self.current_class = class.name.clone();

        // Emit class declaration
        if class.is_throwable {
            self.emitln(&format!("class {} : public std::runtime_error {{\npublic:", class.name));
            self.emitln(&format!("    {}(const std::string& _msg = \"\") : std::runtime_error(\"{}: \" + _msg) {{}}", class.name, class.name));
        } else if let Some(parent) = &class.parent {
            self.emitln(&format!("class {} : public {} {{\nprivate:", class.name, parent));
        } else {
            self.emitln(&format!("class {} {{\nprivate:", class.name));
        }

        let mut current_access = if class.is_throwable { "public" } else { "private" };

        for member in &class.members {
            let member_access = match member {
                ClassMember::Field(f) => if matches!(f.access, AccessModifier::Public) { "public" } else { "private" },
                ClassMember::Method(m) => if matches!(m.access, AccessModifier::Public) { "public" } else { "private" },
                ClassMember::Constructor(_) => "public",
            };
            if member_access != current_access {
                current_access = member_access;
                self.emitln(&format!("{}:", current_access));
            }
            match member {
                ClassMember::Field(f) => self.gen_field(f),
                ClassMember::Method(m) => self.gen_method(m, &class.name),
                ClassMember::Constructor(c) => self.gen_constructor(c, &class.name, class.is_throwable, class.parent.as_deref()),
            }
        }
        self.emitln("};\n");
    }

    fn gen_field(&mut self, field: &FieldDecl) {
        let static_prefix = if field.is_static { "static " } else { "" };
        let cpp_t = self.cpp_type(&field.typ, field.is_mut);
        if let Some(val) = &field.value {
            let val_str = self.gen_expr(val);
            self.emitln(&format!("    {}{} {} = {};", static_prefix, cpp_t, field.name, val_str));
        } else {
            self.emitln(&format!("    {}{} {};", static_prefix, cpp_t, field.name));
        }
    }

    fn gen_constructor(&mut self, ctor: &ConstructorDecl, class_name: &str, is_throwable: bool, parent: Option<&str>) {
        let params = self.gen_params(&ctor.params);
        let init = if is_throwable {
            let msg_param = ctor.params.iter().find(|p| matches!(p.typ, H20Type::StringType)).map(|p| p.name.as_str()).unwrap_or("\"\"");
            format!(" : std::runtime_error({})", msg_param)
        } else if let Some(args) = &ctor.super_args {
            if let Some(parent_name) = parent {
                let args_str: Vec<String> = args.iter().map(|a| self.gen_expr(a)).collect();
                format!(" : {}({})", parent_name, args_str.join(", "))
            } else { String::new() }
        } else { String::new() };
        self.emitln(&format!("    {}({}){} {{", class_name, params, init));
        self.reset_release_tracking();
        for stmt in &ctor.body {
            self.gen_stmt(stmt, "        ");
        }
        if !self.in_unsafe {
            self.check_all_released(class_name, class_name);
        }
        self.reset_release_tracking();
        self.emitln("    }");
    }

    fn gen_method(&mut self, method: &MethodDecl, class_name: &str) {
        if method.name.is_empty() {
            for stmt in &method.body {
                self.gen_stmt(stmt, "    ");
            }
            return;
        }
        let static_prefix = if method.is_static { "static " } else { "" };
        let is_main = method.name == "main" && method.is_static;
        let (ret_type, params) = if is_main {
            ("int".to_string(), "int argc, char* argv[]".to_string())
        } else {
            (self.cpp_type_bare(&method.return_type), self.gen_params(&method.params))
        };
        self.emitln(&format!("    {}{} {}({}) {{", static_prefix, ret_type, method.name, params));
        self.reset_release_tracking();
        for stmt in &method.body {
            self.gen_stmt(stmt, "        ");
        }
        if !self.in_unsafe {
            self.check_all_released(class_name, &method.name);
        }
        self.reset_release_tracking();
        self.emitln("    }");
    }

    fn gen_params(&self, params: &[Param]) -> String {
        params.iter()
            .map(|p| {
                if p.name.is_empty() && matches!(&p.typ, H20Type::Custom(s) if s == "...") {
                    "...".to_string()
                } else {
                    format!("{} {}", self.cpp_param_type(&p.typ, p.is_mut), p.name)
                }
            })
            .collect::<Vec<_>>()
            .join(", ")
    }

    fn gen_stmt(&mut self, stmt: &Stmt, indent: &str) {
        match stmt {
            Stmt::VarDecl { is_mut, is_static, typ, name, value } => {
                let static_prefix = if *is_static { "static " } else { "" };
                let cpp_t = self.cpp_type(typ, *is_mut);
                if !self.in_unsafe {
                    if !self.current_method_vars.iter().any(|(n, _)| n == name) {
                        self.current_method_vars.push((name.clone(), 0));
                        self.local_var_types.insert(name.clone(), typ.clone());
                    }
                }
                if let Some(val) = value {
                    let val_str = self.gen_expr(val);
                    self.emitln(&format!("{}{}{} {} = {};", indent, static_prefix, cpp_t, name, val_str));
                } else {
                    self.emitln(&format!("{}{}{} {};", indent, static_prefix, cpp_t, name));
                }
            }
            Stmt::Return(expr) => {
                if let Some(e) = expr {
                    let s = self.gen_expr(e);
                    self.emitln(&format!("{}return {};", indent, s));
                } else {
                    self.emitln(&format!("{}return;", indent));
                }
            }
            Stmt::Throw(expr) => {
                let s = self.gen_expr(expr);
                self.emitln(&format!("{}throw {};", indent, s));
            }
            Stmt::If { condition, then_block, elif_blocks, else_block } => {
                let cond = self.gen_expr(condition);
                self.emitln(&format!("{}if ({}) {{", indent, cond));
                for s in then_block { self.gen_stmt(s, &format!("{}    ", indent)); }
                self.emitln(&format!("{}}}", indent));
                for (cond, body) in elif_blocks {
                    let c = self.gen_expr(cond);
                    self.emitln(&format!("{} else if ({}) {{", indent, c));
                    for s in body { self.gen_stmt(s, &format!("{}    ", indent)); }
                    self.emitln(&format!("{}}}", indent));
                }
                if let Some(body) = else_block {
                    self.emitln(&format!("{} else {{", indent));
                    for s in body { self.gen_stmt(s, &format!("{}    ", indent)); }
                    self.emitln(&format!("{}}}", indent));
                }
            }
            Stmt::While { condition, body } => {
                let cond = self.gen_expr(condition);
                self.emitln(&format!("{}while ({}) {{", indent, cond));
                for s in body { self.gen_stmt(s, &format!("{}    ", indent)); }
                self.emitln(&format!("{}}}", indent));
            }
            Stmt::For { init, condition, update, body } => {
                let init_str = if let Some(s) = init {
                    match s.as_ref() {
                        Stmt::VarDecl { is_mut, typ, name, value, .. } => {
                            let cpp_t = self.cpp_type(typ, *is_mut);
                            if let Some(v) = value { format!("{} {} = {}", cpp_t, name, self.gen_expr(v)) }
                            else { format!("{} {}", cpp_t, name) }
                        }
                        Stmt::_Expr(e) => self.gen_expr(e),
                        _ => String::new(),
                    }
                } else { String::new() };
                let cond_str = condition.as_ref().map(|e| self.gen_expr(e)).unwrap_or_default();
                let upd_str = update.as_ref().map(|e| self.gen_expr(e)).unwrap_or_default();
                self.emitln(&format!("{}for ({}; {}; {}) {{", indent, init_str, cond_str, upd_str));
                for s in body { self.gen_stmt(s, &format!("{}    ", indent)); }
                self.emitln(&format!("{}}}", indent));
            }
            Stmt::Break => self.emitln(&format!("{}break;", indent)),
            Stmt::Continue => self.emitln(&format!("{}continue;", indent)),
            Stmt::Release(name) => {
                if let Some(typ) = self.local_var_types.get(name).cloned() {
                    if let Some(zero) = self.zero_value(&typ) {
                        self.emitln(&format!("{}{} = {}; // released", indent, name, zero));
                    }
                }
                self.released_vars.insert(name.clone());
                self.current_method_vars.retain(|(n, _)| n != name);
                self.local_var_types.remove(name);
            }
            Stmt::Zombify(name) => {
                if !self.current_method_vars.iter().any(|(n, _)| n == name) {
                    self.error(&format!("cannot zombify '{}'. it doesn't exist", name));
                }
                if self.released_vars.contains(name) {
                    self.error(&format!("cannot zombify '{}'. it is already released", name));
                }
                self.zombie_vars.insert(name.clone());
            }
            Stmt::Unsafe(body) => {
                self.in_unsafe = true;
                self.emitln(&format!("{}{{  // unsafe", indent));
                for s in body { self.gen_stmt(s, &format!("{}    ", indent)); }
                self.emitln(&format!("{}}}", indent));
                self.in_unsafe = false;
            }
            Stmt::_Expr(expr) => {
                let s = self.gen_expr(expr);
                self.emitln(&format!("{}{};", indent, s));
            }
            Stmt::RawCpp(s) => {
                self.emitln(&format!("{}{}", indent, s));
            }
        }
    }

    fn gen_expr(&mut self, expr: &Expr) -> String {
        match expr {
            Expr::IntLiteral(n) => n.to_string(),
            Expr::FloatLiteral(f) => format!("{}", f),
            Expr::BoolLiteral(b) => if *b { "true".to_string() } else { "false".to_string() },
            Expr::StringLiteral(s) => format!("\"{}\"", s),
            Expr::Null => "nullptr".to_string(),
            Expr::Ident(name) => name.clone(),
            Expr::BinaryOp(left, op, right) => {
                let l = self.gen_expr(left);
                let r = self.gen_expr(right);
                let op_str = match op {
                    BinOp::Add => "+", BinOp::Sub => "-", BinOp::Mul => "*", BinOp::Div => "/",
                    BinOp::Mod => "%", BinOp::And => "&&", BinOp::Or => "||", BinOp::Eq => "==",
                    BinOp::NotEq => "!=", BinOp::Lt => "<", BinOp::Gt => ">", BinOp::LtEq => "<=",
                    BinOp::GtEq => ">=", BinOp::Assign => "=",
                };
                format!("{} {} {}", l, op_str, r)
            }
            Expr::UnaryOp(op, expr) => {
                let e = self.gen_expr(expr);
                match op {
                    UnaryOp::Not => format!("!{}", e),
                    UnaryOp::Neg => format!("-{}", e),
                    UnaryOp::Deref => format!("*{}", e),
                }
            }
            Expr::StaticCall(class, member, args, has_parens) => {
                if *has_parens || !args.is_empty() {
                    let args_str: Vec<String> = args.iter().map(|a| self.gen_expr(a)).collect();
                    format!("{}::{}({})", class, member, args_str.join(", "))
                } else {
                    format!("{}::{}", class, member)
                }
            }
            Expr::MethodCall(obj, method, args) => {
                let args_str: Vec<String> = args.iter().map(|a| self.gen_expr(a)).collect();
                match obj.as_ref() {
                    Expr::Ident(name) if name == "this" => format!("{}({})", method, args_str.join(", ")),
                    _ => format!("{}.{}({})", self.gen_expr(obj), method, args_str.join(", ")),
                }
            }
            Expr::FieldAccess(obj, field) => format!("{}.{}", self.gen_expr(obj), field),
            Expr::NewObject(class_name, args) => {
                let args_str: Vec<String> = args.iter().map(|a| self.gen_expr(a)).collect();
                format!("{}({})", class_name, args_str.join(", "))
            }
            Expr::NewList(inner) => format!("std::make_shared<std::vector<{}>>()", self.cpp_type_bare(inner)),
            Expr::ListOf(inner, args) => {
                let inner_cpp = self.cpp_type_bare(inner);
                let args_str: Vec<String> = args.iter().map(|a| self.gen_expr(a)).collect();
                format!("std::make_shared<const std::vector<{}>>(std::vector<{}>{{{}}})", inner_cpp, inner_cpp, args_str.join(", "))
            }
            Expr::_Format(fmt, args) => {
                let args_str: Vec<String> = args.iter().map(|a| self.gen_expr(a)).collect();
                if args_str.is_empty() {
                    format!("\"{}\"", fmt)
                } else {
                    format!("std::format(\"{}\", {})", fmt, args_str.join(", "))
                }
            }
        }
    }

    fn reset_release_tracking(&mut self) {
        self.current_method_vars.clear();
        self.local_var_types.clear();
        self.released_vars.clear();
        self.zombie_vars.clear();
        self.in_unsafe = false;
    }

    fn check_all_released(&self, class_name: &str, method_name: &str) {
        for (var_name, _) in &self.current_method_vars {
            if !self.released_vars.contains(var_name) && !self.zombie_vars.contains(var_name) {
                eprintln!("{}{}: variable '{}' from method '{}' was not released{}", ANSI_RED, class_name, var_name, method_name, ANSI_RESET);
                std::process::exit(1);
            }
        }
    }
}