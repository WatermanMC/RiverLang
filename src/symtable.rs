// last updated: may 24, 2026
#[derive(Clone, Copy, PartialEq)]
pub enum Access {
    Public,
    Private,
}

#[derive(Clone)]
pub struct Field {
    pub name: String,
    pub typ: String,
    pub access: Access,
}

#[derive(Clone)]
pub struct Method {
    pub name: String,
    pub _signature: String,
    pub ret: String,
    pub access: Access,
}

#[derive(Clone)]
pub struct ClassInfo {
    pub name: String,
    pub path: String,
    pub fields: Vec<Field>,
    pub methods: Vec<Method>,
    pub imports: Vec<String>,
    pub is_throwable: bool,
}