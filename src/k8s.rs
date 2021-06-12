pub mod lock;
pub use lock::*;

pub mod api_resource;
pub use api_resource::*;

pub mod transaction;
pub use transaction::*;

use crate::meta::CRATE_VERSION;
use kube::ResourceExt;
use serde::Serialize;
use std::fmt::Debug;

pub const TYPE_LABEL: &'static str = "able-seaman/type";
pub const VERSION_LABEL: &'static str = "able-seaman/version";

#[derive(Clone, Copy, Debug, Serialize)]
pub enum ObjectType {
    Lock,
    Release,
}

impl ToString for ObjectType {
    fn to_string(&self) -> String {
        match self {
            ObjectType::Lock => "lock",
            ObjectType::Release => "release",
        }
        .to_string()
    }
}

pub trait TaggableObject {
    fn tag(&mut self, object_type: ObjectType);
}

impl<SomeResource: ResourceExt> TaggableObject for SomeResource {
    fn tag(&mut self, object_type: ObjectType) {
        self.labels_mut()
            .insert(TYPE_LABEL.to_string(), object_type.to_string());

        self.annotations_mut()
            .insert(VERSION_LABEL.to_string(), CRATE_VERSION.to_string());
    }
}
