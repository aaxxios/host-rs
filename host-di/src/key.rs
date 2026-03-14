use std::any::{TypeId, type_name};
use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hash, Hasher};

/// Identity of a service type, carrying its name for diagnostics.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Key {
    pub(crate) id: TypeId,
    pub(crate) name: &'static str,
}

impl Key {
    pub(crate) fn of<T: ?Sized + 'static>() -> Self {
        Self {
            id: TypeId::of::<T>(),
            name: type_name::<T>(),
        }
    }
}

impl PartialEq for Key {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for Key {}

impl Hash for Key {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

/// `TypeId` is already a hash, so its `write_u64` is passed through unchanged.
pub(crate) struct TypeIdHasher(u64);

impl Default for TypeIdHasher {
    fn default() -> Self {
        Self(0xcbf2_9ce4_8422_2325)
    }
}

impl Hasher for TypeIdHasher {
    fn write(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.0 ^= u64::from(byte);
            self.0 = self.0.wrapping_mul(0x0100_0000_01b3);
        }
    }

    fn write_u64(&mut self, n: u64) {
        self.0 = n;
    }

    fn finish(&self) -> u64 {
        self.0
    }
}

pub(crate) type KeyMap<V> = HashMap<Key, V, BuildHasherDefault<TypeIdHasher>>;

/// A service another service needs in order to be constructed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Dependency {
    key: Key,
}

impl Dependency {
    /// A dependency on a single instance of `T`.
    pub fn on<T: ?Sized + 'static>() -> Self {
        Self {
            key: Key::of::<T>(),
        }
    }

    /// The dependency's type name.
    pub fn type_name(&self) -> &'static str {
        self.key.name
    }
}
