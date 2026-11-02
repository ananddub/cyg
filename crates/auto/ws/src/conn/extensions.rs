use std::any::{Any, TypeId};
use std::collections::HashMap;

use uuid::Uuid;

#[derive(Default)]
pub struct Extensions {
    values: HashMap<TypeId, Box<dyn Any + Send + Sync>>,
}

impl Extensions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert<T: Any + Send + Sync>(&mut self, value: T) {
        self.values.insert(TypeId::of::<T>(), Box::new(value));
    }

    pub fn get<T: Any + Send + Sync>(&self) -> Option<&T> {
        self.values.get(&TypeId::of::<T>())?.downcast_ref::<T>()
    }
}

pub struct Identity {
    pub user_id: Uuid,
    pub username: String,
    pub data: Extensions,
}

impl Identity {
    pub fn new(user_id: Uuid, username: impl Into<String>) -> Self {
        Self {
            user_id,
            username: username.into(),
            data: Extensions::new(),
        }
    }

    pub fn with<T: Any + Send + Sync>(mut self, value: T) -> Self {
        self.data.insert(value);
        self
    }
}
