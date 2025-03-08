use std::{
    any::{type_name, TypeId},
    cmp::Ordering,
    fmt, hash,
};

#[derive(Copy, Clone)]
pub struct NamedTypeId {
    id: TypeId,
    name: &'static str,
}

impl NamedTypeId {
    pub fn of<T: 'static>() -> Self {
        Self {
            id: TypeId::of::<T>(),
            name: type_name::<T>(),
        }
    }

    pub fn id(self) -> TypeId {
        self.id
    }

    pub fn name(self) -> &'static str {
        self.name
    }
}

impl fmt::Debug for NamedTypeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("NamedTypeId")
            .field(&self.name)
            .field(&self.id)
            .finish()
    }
}

impl fmt::Display for NamedTypeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "`{}` ({:?})", self.name, self.id)
    }
}

impl hash::Hash for NamedTypeId {
    fn hash<H: hash::Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

impl Eq for NamedTypeId {}

impl PartialEq for NamedTypeId {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Ord for NamedTypeId {
    fn cmp(&self, other: &Self) -> Ordering {
        self.id.cmp(&other.id)
    }
}

impl PartialOrd for NamedTypeId {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
