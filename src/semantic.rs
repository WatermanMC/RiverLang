// last updated: june 2, 2026
use crate::ast::*;
use crate::symtable::ClassInfo;
use std::collections::HashMap;

#[derive(Debug)]
pub struct SemanticError {
    pub message: String,
    pub line: usize,
}

impl SemanticError {
    fn new(msg: &str, line: usize) -> Self {
        Self {
            message: msg.to_string(),
            line,
        }
    }
}

#[derive(Debug, Clone)]
struct VarInfo {
    typ: H20Type,
    is_mut: bool,
    is_zombie: bool,
    line: usize,
}

struct Scope {
    vars: HashMap<String, VarInfo>,
}

impl Scope {
    fn new() -> Self {
        Self {
            vars: HashMap::new(),
        }
    }
}

pub struct SemanticAnalyzer<'a> {
    class_registry: &'a [ClassInfo],
    errors: Vec<SemanticError>,
    current_class: String,
    current_class_is_throwable: bool,
    current_method_return_type: H20Type,
    current_method_name: String,
    imported_classes: Vec<String>,
    in_unsafe: bool,
    scopes: Vec<Scope>,
}

impl<'a> SemanticAnalyzer<'a> {
    pub fn new(class_registry: &'a [ClassInfo]) -> Self {
        Self {
            class_registry,
            errors: Vec::new(),
            current_class: String::new(),
            current_class_is_throwable: false,
            current_method_return_type: H20Type::Void,
            current_method_name: String::new(),
            imported_classes: Vec::new(),
            in_unsafe: false,
            scopes: Vec::new(),
        }
    }

    fn types_compatible(&self, expected: &H20Type, actual: &H20Type) -> bool {
        match (expected, actual) {
            (H20Type::Void, _) => true,
            (H20Type::Int, H20Type::Int) => true,
            (H20Type::Float, H20Type::Float) => true,
            (H20Type::Float, H20Type::Int) => true,
            (H20Type::Boolean, H20Type::Boolean) => true,
            (H20Type::StringType, H20Type::StringType) => true,
            (H20Type::Custom(a), H20Type::Custom(b)) => a == b,
            (H20Type::List(a), H20Type::List(b)) => self.types_compatible(a, b),
            _ => false,
        }
    }

    fn error(&mut self, msg: &str, line: usize) {
        self.errors.push(SemanticError::new(msg, line));
    }

    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }

    pub fn print_errors(&self, filename: &str) {
        for e in &self.errors {
            eprintln!(
                "\x1b[31m{}:{}: error: {}\x1b[0m",
                filename, e.line, e.message
            );
        }
    }

    fn push_scope(&mut self) {
        self.scopes.push(Scope::new());
    }

    fn pop_scope(&mut self) {
        if !self.in_unsafe {
            if let Some(scope) = self.scopes.last() {
                for (name, info) in &scope.vars {
                    if !info.is_zombie {
                        self.errors.push(SemanticError::new(
                            &format!("variable '{}' was never released", name),
                            info.line,
                        ));
                    }
                }
            }
        }
        self.scopes.pop();
    }

    fn declare_var(&mut self, name: String, info: VarInfo) {
        if let Some(scope) = self.scopes.last() {
            if scope.vars.contains_key(&name) {
                let line = info.line;
                self.error(
                    &format!("variable '{}' already declared in this scope", name),
                    line,
                );
                return;
            }
        }
        if let Some(scope) = self.scopes.last_mut() {
            scope.vars.insert(name, info);
        }
    }

    fn lookup_var(&self, name: &str) -> Option<&VarInfo> {
        for scope in self.scopes.iter().rev() {
            if let Some(info) = scope.vars.get(name) {
                return Some(info);
            }
        }
        None
    }

    fn lookup_var_mut(&mut self, name: &str) -> Option<&mut VarInfo> {
        for scope in self.scopes.iter_mut().rev() {
            if scope.vars.contains_key(name) {
                return scope.vars.get_mut(name);
            }
        }
        None
    }

    fn remove_var(&mut self, name: &str) {
        for scope in self.scopes.iter_mut().rev() {
            if scope.vars.remove(name).is_some() {
                return;
            }
        }
    }

    fn find_class(&self, name: &str) -> Option<&ClassInfo> {
        self.class_registry.iter().find(|c| c.name == name)
    }

    fn is_throwable(&self, name: &str) -> bool {
        self.find_class(name)
            .map(|c| c.is_throwable)
            .unwrap_or(false)
    }

    fn class_exists(&self, name: &str) -> bool {
        self.find_class(name).is_some()
    }

    fn is_imported(&self, class_name: &str) -> bool {
        if let Some(info) = self.find_class(class_name) {
            if info.path.starts_with("C:/RDK") {
                return true;
            }
        }
        if class_name == self.current_class {
            return true;
        }
        self.imported_classes.iter().any(|imp| {
            imp.split('/').last().unwrap_or(imp) == class_name
                || imp.split(':').last().unwrap_or(imp) == class_name
        })
    }

    fn member_is_accessible(&self, class_name: &str, member_name: &str) -> bool {
        if class_name == self.current_class {
            return true;
        }
        if let Some(info) = self.find_class(class_name) {
            for field in &info.fields {
                if field.name == member_name {
                    return matches!(field.access, crate::symtable::Access::Public);
                }
            }
            for method in &info.methods {
                if method.name == member_name {
                    return matches!(method.access, crate::symtable::Access::Public);
                }
            }
        }
        true
    }

    fn infer_expr_type(&self, expr: &Expr) -> H20Type {
        match expr {
            Expr::IntLiteral(_) => H20Type::Int,
            Expr::FloatLiteral(_) => H20Type::Float,
            Expr::BoolLiteral(_) => H20Type::Boolean,
            Expr::StringLiteral(_) => H20Type::StringType,
            Expr::_Format(_, _) => H20Type::StringType,
            Expr::Null => H20Type::Custom("null".to_string()),

            Expr::Ident(name) => {
                if let Some(info) = self.lookup_var(name) {
                    return info.typ.clone();
                }
                H20Type::Custom(name.clone())
            }

            Expr::BinaryOp(left, op, _) => match op {
                BinOp::Eq
                | BinOp::NotEq
                | BinOp::Lt
                | BinOp::Gt
                | BinOp::LtEq
                | BinOp::GtEq
                | BinOp::And
                | BinOp::Or => H20Type::Boolean,
                BinOp::Assign => self.infer_expr_type(left),
                _ => self.infer_expr_type(left),
            },

            Expr::UnaryOp(op, expr) => match op {
                UnaryOp::Not => H20Type::Boolean,
                UnaryOp::Neg => self.infer_expr_type(expr),
                UnaryOp::Deref => self.infer_expr_type(expr),
            },

            Expr::NewObject(class_name, _) => H20Type::Custom(class_name.clone()),
            Expr::NewList(inner) => H20Type::List(Box::new(inner.clone())),
            Expr::ListOf(inner, _) => H20Type::List(Box::new(inner.clone())),

            Expr::StaticCall(class, method, _, _) => {
                if let Some(info) = self.find_class(class) {
                    if let Some(m) = info.methods.iter().find(|m| m.name == *method) {
                        return H20Type::Custom(m.ret.clone());
                    }
                }
                H20Type::Void
            }

            Expr::MethodCall(obj, method, _) => {
                let obj_type = self.infer_expr_type(obj);
                if let H20Type::Custom(ref class_name) = obj_type {
                    if let Some(info) = self.find_class(class_name) {
                        if let Some(m) = info.methods.iter().find(|m| m.name == *method) {
                            return Self::str_to_h20type(&m.ret);
                        }
                    }
                }
                H20Type::Void
            }

            Expr::FieldAccess(obj, field) => {
                let obj_type = self.infer_expr_type(obj);
                if let H20Type::Custom(ref class_name) = obj_type {
                    if let Some(info) = self.find_class(class_name) {
                        if let Some(f) = info.fields.iter().find(|f| f.name == *field) {
                            return Self::str_to_h20type(&f.typ);
                        }
                    }
                }
                H20Type::Void
            }
        }
    }

    fn str_to_h20type(s: &str) -> H20Type {
        match s {
            "int" => H20Type::Int,
            "float" => H20Type::Float,
            "boolean" => H20Type::Boolean,
            "String" => H20Type::StringType,
            "void" => H20Type::Void,
            other => H20Type::Custom(other.to_string()),
        }
    }

    pub fn analyze(&mut self, program: &Program, filename: &str) {
        for item in &program.items {
            if let TopLevel::Import(path) = item {
                self.imported_classes.push(path.clone());
            }
        }
        for item in &program.items {
            self.analyze_top_level(item, filename);
        }
    }

    fn analyze_top_level(&mut self, item: &TopLevel, filename: &str) {
        match item {
            TopLevel::Class(class) => self.analyze_class(class, filename),
            TopLevel::Import(_) | TopLevel::Include(_) | TopLevel::RawCpp(_) => {}
        }
    }

    fn analyze_class(&mut self, class: &ClassDecl, _filename: &str) {
        self.current_class = class.name.clone();
        self.current_class_is_throwable = class.is_throwable;

        if let Some(parent) = &class.parent {
            if !self.class_exists(parent) {
                self.error(&format!("parent class '{}' does not exist", parent), 0);
            }
        }

        let mut member_names: Vec<String> = Vec::new();
        for member in &class.members {
            let name = match member {
                ClassMember::Field(f) => f.name.clone(),
                ClassMember::Method(m) => m.name.clone(),
                ClassMember::Constructor(_) => "__constructor__".to_string(),
            };
            if !name.is_empty() && name != "__constructor__" && member_names.contains(&name) {
                self.error(
                    &format!("duplicate member '{}' in class '{}'", name, class.name),
                    0,
                );
            }
            if !name.is_empty() {
                member_names.push(name);
            }
        }

        for member in &class.members {
            match member {
                ClassMember::Field(f) => self.analyze_field(f),
                ClassMember::Method(m) => self.analyze_method(m, &class.name.clone()),
                ClassMember::Constructor(c) => self.analyze_constructor(c, class),
            }
        }
    }

    fn analyze_field(&mut self, field: &FieldDecl) {
        if !field.is_mut && field.value.is_none() {
            self.error(
                &format!(
                    "immutable field '{}' must have an initial value",
                    field.name
                ),
                0,
            );
        }
        if let H20Type::Custom(ref type_name) = field.typ {
            if !self.class_exists(type_name) {
                self.error(
                    &format!("unknown type '{}' for field '{}'", type_name, field.name),
                    0,
                );
            }
        }
        if let Some(val) = &field.value.clone() {
            self.push_scope();
            self.analyze_expr(&val, 0);
            self.scopes.pop();
        }
    }

    fn analyze_method(&mut self, method: &MethodDecl, _class_name: &str) {
        if method.name.is_empty() {
            return;
        }

        self.current_method_return_type = method.return_type.clone();
        self.current_method_name = method.name.clone();

        if let H20Type::Custom(ref type_name) = method.return_type {
            if type_name != "void" && !self.class_exists(type_name) {
                self.error(
                    &format!(
                        "unknown return type '{}' for method '{}'",
                        type_name, method.name
                    ),
                    0,
                );
            }
        }

        self.push_scope();

        for param in &method.params {
            self.declare_var(
                param.name.clone(),
                VarInfo {
                    typ: param.typ.clone(),
                    is_mut: param.is_mut,
                    is_zombie: true,
                    line: 0,
                },
            );
        }

        for stmt in &method.body {
            self.analyze_stmt(stmt, 0);
        }

        self.pop_scope();
    }

    fn analyze_constructor(&mut self, ctor: &ConstructorDecl, _class: &ClassDecl) {
        self.current_method_return_type = H20Type::Void;
        self.current_method_name = "constructor".to_string();

        if let Some(args) = &ctor.super_args.clone() {
            for arg in args {
                self.push_scope();
                self.analyze_expr(&arg, 0);
                self.scopes.pop();
            }
        }

        self.push_scope();

        for param in &ctor.params {
            self.declare_var(
                param.name.clone(),
                VarInfo {
                    typ: param.typ.clone(),
                    is_mut: param.is_mut,
                    is_zombie: true,
                    line: 0,
                },
            );
        }

        for stmt in &ctor.body {
            self.analyze_stmt(stmt, 0);
        }

        self.pop_scope();
    }

    fn analyze_stmt(&mut self, stmt: &Stmt, line: usize) {
        match stmt {
            Stmt::VarDecl {
                is_mut,
                typ,
                name,
                value,
                ..
            } => {
                if let H20Type::Custom(type_name) = typ {
                    if !self.class_exists(type_name) && type_name != "null" {
                        self.error(
                            &format!("unknown type '{}' for variable '{}'", type_name, name),
                            line,
                        );
                    }
                    if !self.is_imported(type_name) && self.class_exists(type_name) {
                        self.error(
                            &format!("class '{}' is used but not imported", type_name),
                            line,
                        );
                    }
                }

                if !is_mut && value.is_none() {
                    self.error(
                        &format!("immutable variable '{}' must be initialized", name),
                        line,
                    );
                }

                if let Some(val) = value {
                    self.analyze_expr(val, line);
                    let actual = self.infer_expr_type(val);
                    if !matches!(actual, H20Type::Void) {
                        if !self.types_compatible(typ, &actual) {
                            self.error(
                                &format!(
                                    "type mismatch: expected '{:?}' but got '{:?}'",
                                    typ, actual
                                ),
                                line,
                            );
                        }
                    }
                }

                self.declare_var(
                    name.clone(),
                    VarInfo {
                        typ: typ.clone(),
                        is_mut: *is_mut,
                        is_zombie: false,
                        line,
                    },
                );
            }

            Stmt::Return(expr) => {
                if let Some(e) = expr {
                    self.analyze_expr(e, line);
                    let actual = self.infer_expr_type(e);
                    if !matches!(actual, H20Type::Void)
                        && !matches!(self.current_method_return_type, H20Type::Void)
                    {
                        if !self.types_compatible(&self.current_method_return_type.clone(), &actual)
                        {
                            self.error(
                                &format!(
                                    "return type mismatch in '{}': expected '{:?}' but got '{:?}'",
                                    self.current_method_name,
                                    self.current_method_return_type,
                                    actual
                                ),
                                line,
                            );
                        }
                    }
                }
            }

            Stmt::Throw(expr) => {
                self.analyze_expr(expr, line);
                match expr {
                    Expr::NewObject(class_name, _) | Expr::Ident(class_name) => {
                        if self.class_exists(class_name) && !self.is_throwable(class_name) {
                            self.error(
                                &format!(
                                    "'{}' is not throwable. declare it using 'public throwable {} {{ ...'",
                                    class_name, class_name
                                ),
                                line,
                            );
                        }
                    }
                    _ => {}
                }
            }

            Stmt::If {
                condition,
                then_block,
                elif_blocks,
                else_block,
            } => {
                self.analyze_expr(condition, line);

                let cond_type = self.infer_expr_type(condition);
                if !matches!(cond_type, H20Type::Boolean | H20Type::Custom(_)) {
                    self.error("if condition must be a boolean expression", line);
                }

                self.push_scope();
                for s in then_block {
                    self.analyze_stmt(s, line);
                }
                self.pop_scope();

                for (cond, body) in elif_blocks {
                    self.analyze_expr(cond, line);
                    self.push_scope();
                    for s in body {
                        self.analyze_stmt(s, line);
                    }
                    self.pop_scope();
                }

                if let Some(body) = else_block {
                    self.push_scope();
                    for s in body {
                        self.analyze_stmt(s, line);
                    }
                    self.pop_scope();
                }
            }

            Stmt::While { condition, body } => {
                self.analyze_expr(condition, line);
                self.push_scope();
                for s in body {
                    self.analyze_stmt(s, line);
                }
                self.pop_scope();
            }

            Stmt::For {
                init,
                condition,
                update,
                body,
            } => {
                self.push_scope();
                if let Some(s) = init {
                    self.analyze_stmt(s, line);
                }
                if let Some(e) = condition {
                    self.analyze_expr(e, line);
                }
                if let Some(e) = update {
                    self.analyze_expr(e, line);
                }
                for s in body {
                    self.analyze_stmt(s, line);
                }
                self.pop_scope();
            }

            Stmt::Release(name) => {
                if self.in_unsafe {
                    self.error("'release' is not allowed inside unsafe blocks", line);
                    return;
                }
                match self.lookup_var(name) {
                    None => {
                        self.error(
                            &format!(
                                "cannot release '{}'. variable not declared in this scope",
                                name
                            ),
                            line,
                        );
                    }
                    Some(info) if info.is_zombie => {
                        self.error(&format!("cannot release '{}'. its zombified", name), line);
                    }
                    _ => {
                        self.remove_var(name);
                    }
                }
            }

            Stmt::Zombify(name) => {
                if self.in_unsafe {
                    self.error("'zombify' is not allowed inside unsafe blocks", line);
                    return;
                }
                match self.lookup_var(name) {
                    None => {
                        self.error(
                            &format!(
                                "cannot zombify '{}'. variable not declared in this scope",
                                name
                            ),
                            line,
                        );
                    }
                    _ => {
                        if let Some(info) = self.lookup_var_mut(name) {
                            info.is_zombie = true;
                        }
                    }
                }
            }

            Stmt::Unsafe(body) => {
                self.in_unsafe = true;
                self.push_scope();
                for s in body {
                    self.analyze_stmt(s, line);
                }
                self.scopes.pop();
                self.in_unsafe = false;
            }

            Stmt::_Expr(expr) => {
                self.analyze_expr(expr, line);

                if let Expr::BinaryOp(left, BinOp::Assign, _) = expr {
                    if let Expr::Ident(name) = left.as_ref() {
                        if let Some(info) = self.lookup_var(name) {
                            if !info.is_mut {
                                self.error(
                                    &format!("cannot assign to immutable variable '{}'", name),
                                    line,
                                );
                            }
                        }
                    }
                }
            }

            Stmt::Break | Stmt::Continue | Stmt::RawCpp(_) => {}
        }
    }

    fn analyze_expr(&mut self, expr: &Expr, line: usize) {
        match expr {
            Expr::Ident(name) => {
                if self.lookup_var(name).is_none() && !self.class_exists(name) {}
                if self.class_exists(name) && !self.is_imported(name) {
                    self.error(&format!("class '{}' is used but not imported", name), line);
                }
            }

            Expr::BinaryOp(left, op, right) => {
                self.analyze_expr(left, line);
                self.analyze_expr(right, line);

                if matches!(op, BinOp::Assign) {
                    if let Expr::Ident(name) = left.as_ref() {
                        if let Some(info) = self.lookup_var(name) {
                            if !info.is_mut {
                                self.error(
                                    &format!("cannot assign to immutable variable '{}'", name),
                                    line,
                                );
                            }
                        }
                    }
                }
            }

            Expr::UnaryOp(_, expr) => {
                self.analyze_expr(expr, line);
            }

            Expr::MethodCall(obj, method, _) => {
                let obj_type = self.infer_expr_type(obj);
                if let H20Type::Custom(ref class_name) = obj_type {
                    if let Some(info) = self.find_class(class_name) {
                        if let Some(m) = info.methods.iter().find(|m| m.name == *method) {
                            Self::str_to_h20type(&m.ret);
                            return;
                        }
                    }
                }
                drop(H20Type::Void);
            }

            Expr::FieldAccess(obj, field) => {
                let obj_type = self.infer_expr_type(obj);
                if let H20Type::Custom(ref class_name) = obj_type {
                    if let Some(info) = self.find_class(class_name) {
                        if let Some(f) = info.fields.iter().find(|f| f.name == *field) {
                            Self::str_to_h20type(&f.typ);
                            return;
                        }
                    }
                }
                drop(H20Type::Void);
            }

            Expr::StaticCall(class, method, args, _) => {
                if self.class_exists(class) && !self.is_imported(class) {
                    self.error(&format!("class '{}' is used but not imported", class), line);
                }
                if !self.member_is_accessible(class, method) {
                    self.error(
                        &format!("'{}' has private access in '{}'", method, class),
                        line,
                    );
                }
                for arg in args {
                    self.analyze_expr(arg, line);
                }
            }

            Expr::NewObject(class_name, args) => {
                if !self.class_exists(class_name) {
                    self.error(&format!("unknown class '{}'", class_name), line);
                } else if !self.is_imported(class_name) {
                    self.error(
                        &format!("class '{}' is used but not imported", class_name),
                        line,
                    );
                }
                for arg in args {
                    self.analyze_expr(arg, line);
                }
            }

            Expr::NewList(inner) => {
                if let H20Type::Custom(type_name) = inner {
                    if !self.class_exists(type_name) {
                        self.error(&format!("unknown type '{}' in List", type_name), line);
                    }
                }
            }

            Expr::ListOf(inner, args) => {
                if let H20Type::Custom(type_name) = inner {
                    if !self.class_exists(type_name) {
                        self.error(&format!("unknown type '{}' in List.of", type_name), line);
                    }
                }
                for arg in args {
                    self.analyze_expr(arg, line);
                }
            }

            Expr::_Format(_, args) => {
                for arg in args {
                    self.analyze_expr(arg, line);
                }
            }

            Expr::IntLiteral(_)
            | Expr::FloatLiteral(_)
            | Expr::BoolLiteral(_)
            | Expr::StringLiteral(_)
            | Expr::Null => {}
        }
    }
}
