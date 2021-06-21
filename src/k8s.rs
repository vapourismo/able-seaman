pub mod api_resource;
pub mod lock;
pub mod transaction;

use crate::meta::CRATE_VERSION;
use kube::ResourceExt;
use serde::Serialize;
use std::fmt;

pub const TYPE_LABEL: &str = "able-seaman/type";
pub const VERSION_LABEL: &str = "able-seaman/version";

#[derive(Clone, Copy, Debug, Serialize)]
pub enum ObjectType {
    Lock,
    ReleaseState,
    Managed,
}

impl fmt::Display for ObjectType {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        formatter.write_str(match self {
            ObjectType::Lock => "lock",
            ObjectType::ReleaseState => "release-state",
            ObjectType::Managed => "managed",
        })
    }
}

pub trait TaggableObject {
    fn tag(&mut self, object_type: ObjectType);

    fn to_tagged(&self, object_type: ObjectType) -> Self
    where
        Self: Clone,
    {
        let mut new = self.clone();
        new.tag(object_type);
        new
    }
}

impl<SomeResource: ResourceExt> TaggableObject for SomeResource {
    fn tag(&mut self, object_type: ObjectType) {
        self.labels_mut()
            .insert(TYPE_LABEL.to_string(), object_type.to_string());

        self.annotations_mut()
            .insert(VERSION_LABEL.to_string(), CRATE_VERSION.to_string());
    }
}
