use crate::ir::ir_type::TypePtr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AttributeKind {
    StructReturn,
    NonNull,
    Align,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Attribute {
    StructReturn(TypePtr),
    NonNull,
    Align(u32),
}

impl Attribute {
    pub fn kind(&self) -> AttributeKind {
        match self {
            Attribute::StructReturn(_) => AttributeKind::StructReturn,
            Attribute::NonNull => AttributeKind::NonNull,
            Attribute::Align(_) => AttributeKind::Align,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AttributeSet {
    defined: Vec<Attribute>,
}

impl AttributeSet {
    pub fn insert(&mut self, attr: Attribute) -> Option<Attribute> {
        let kind = attr.kind();
        if let Some(slot) = self
            .defined
            .iter_mut()
            .find(|defined| defined.kind() == kind)
        {
            Some(std::mem::replace(slot, attr))
        } else {
            self.defined.push(attr);
            None
        }
    }

    pub fn remove(&mut self, kind: AttributeKind) -> Option<Attribute> {
        self.defined
            .iter()
            .position(|attr| attr.kind() == kind)
            .map(|index| self.defined.remove(index))
    }

    pub fn get(&self, kind: AttributeKind) -> Option<&Attribute> {
        self.defined.iter().find(|attr| attr.kind() == kind)
    }

    pub fn contains(&self, kind: AttributeKind) -> bool {
        self.get(kind).is_some()
    }

    pub fn iter(&self) -> std::slice::Iter<'_, Attribute> {
        self.defined.iter()
    }

    pub fn is_empty(&self) -> bool {
        self.defined.is_empty()
    }

    pub fn struct_return_ty(&self) -> Option<TypePtr> {
        match self.get(AttributeKind::StructReturn) {
            Some(Attribute::StructReturn(ty)) => Some(ty.clone()),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FunctionAttributes {
    pub function: AttributeSet,
    pub ret: AttributeSet,
}
