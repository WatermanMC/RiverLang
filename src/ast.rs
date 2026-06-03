// lst updated: may 29 2026
// Abstract syntax tree (AST): represents the structure of a RiverLang program after parsing

// Access modifier for class members (fields and methods).
#[derive(Debug, Clone, PartialEq)]
pub enum AccessModifier {
    Public,
    Private,
}

// All data types supported by the language.
#[derive(Debug, Clone, PartialEq)]
pub enum H20Type {
    Int,                // 64‑bit integer (maps to C++ long long)
    Float,              // double precision float (maps to C++ double)
    Boolean,            // true/false (maps to C++ bool)
    StringType,         // mutable string (maps to std::string)
    Void,               // no return value
    List(Box<H20Type>), // generic list, e.g. List<Int>
    Custom(String),     // user-defined class name
}

// Expressions: compute values, call functions, access fields, etc
#[derive(Debug, Clone)]
pub enum Expr {
    IntLiteral(i64),
    FloatLiteral(f64),
    BoolLiteral(bool),
    StringLiteral(String),
    Null, // nullptr in C++

    Ident(String), // variable or class name

    // Static method call: Class::method(args)
    // The bool indicates whether parentheses were present in source
    StaticCall(String, String, Vec<Expr>, bool),

    // Instance method call: obj.method(args)
    MethodCall(Box<Expr>, String, Vec<Expr>),

    // Field access: obj.field
    FieldAccess(Box<Expr>, String),

    BinaryOp(Box<Expr>, BinOp, Box<Expr>),
    UnaryOp(UnaryOp, Box<Expr>),

    NewObject(String, Vec<Expr>),   // class constructor call
    ListOf(H20Type, Vec<Expr>),     // List.of<Type>(...)
    NewList(H20Type),               // new List<Type>()

    _Format(String, Vec<Expr>),     // internal: format string (not user‑visible)
}

// Binary operators
#[derive(Debug, Clone)]
pub enum BinOp {
    Add, Sub, Mul, Div, Mod,
    And, Or,
    Eq, NotEq,
    Lt, Gt, LtEq, GtEq,
    Assign,
}

// Unary operators
#[derive(Debug, Clone)]
pub enum UnaryOp {
    Not, // logical not (!)
    Neg, // arithmetic negation (-)
    Deref, // pointer dereference (*)
}

// Statements: actions that do not return a value (declarations, loops, etc.)
#[derive(Debug, Clone)]
pub enum Stmt {
    VarDecl {
        is_mut: bool,
        is_static: bool,
        typ: H20Type,
        name: String,
        value: Option<Expr>,
    },
    If {
        condition: Expr,
        then_block: Vec<Stmt>,
        elif_blocks: Vec<(Expr, Vec<Stmt>)>,
        else_block: Option<Vec<Stmt>>,
    },
    While {
        condition: Expr,
        body: Vec<Stmt>,
    },
    For {
        init: Option<Box<Stmt>>,
        condition: Option<Expr>,
        update: Option<Expr>,
        body: Vec<Stmt>,
    },
    Return(Option<Expr>),
    Break,
    Continue,
    _Expr(Expr),            // expression used as a statement (e.g. function call)
    Throw(Expr),
    Release(String),        // explicitly release a variable (set to zero/null)
    Zombify(String),        // mark variable as intentionally leaked (no release required)
    Unsafe(Vec<Stmt>),      // block of raw C++ code (parsed as statements)
    RawCpp(String),         // raw C++ line (not parsed inside)
}

// Function parameter.
#[derive(Debug, Clone)]
pub struct Param {
    pub is_mut: bool,
    pub typ: H20Type,
    pub name: String,
}

// Class field declaration.
#[derive(Debug, Clone)]
pub struct FieldDecl {
    pub access: AccessModifier,
    pub is_static: bool,
    pub is_mut: bool,
    pub typ: H20Type,
    pub name: String,
    pub value: Option<Expr>,
}

// Class method declaration
#[derive(Debug, Clone)]
pub struct MethodDecl {
    pub access: AccessModifier,
    pub is_static: bool,
    pub name: String,
    pub params: Vec<Param>,
    pub return_type: H20Type,
    pub body: Vec<Stmt>,
}

// Class constructor declaration
#[derive(Debug, Clone)]
pub struct ConstructorDecl {
    pub params: Vec<Param>,
    pub super_args: Option<Vec<Expr>>,
    pub body: Vec<Stmt>,
}

// A member of a class: field, method, or constructor
#[derive(Debug, Clone)]
pub enum ClassMember {
    Field(FieldDecl),
    Method(MethodDecl),
    Constructor(ConstructorDecl),
}

// A class declaration
#[derive(Debug, Clone)]
pub struct ClassDecl {
    pub access: AccessModifier,
    pub is_throwable: bool, // if true, inherits from std::runtime_error
    pub name: String,
    pub parent: Option<String>,
    pub members: Vec<ClassMember>,
}

// Top‑level items in a source file
#[derive(Debug, Clone)]
pub enum TopLevel {
    Import(String), // 'use' path (not fully implemented)
    Include(String), // '#include ...' directive
    Class(ClassDecl),
    RawCpp(String), // raw C++ line at top level
}

// The entire program: a list of top‑level items
#[derive(Debug, Clone)]
pub struct Program {
    pub items: Vec<TopLevel>,
}