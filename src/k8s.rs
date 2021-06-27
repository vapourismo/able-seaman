pub mod annotations;
pub mod api_resource;
pub mod labels;
pub mod lock;
pub mod transaction;

use crate::meta;
use const_format::concatcp;
use serde::Serialize;
use std::fmt;

const VERSION_KEY: &str = concatcp!(meta::CRATE_NAME, "/version");

#[derive(Clone, Copy, Debug)]
pub struct CrateVersion;

impl annotations::ToAnnotation for CrateVersion {
    fn to_annotation(&self) -> (&'static str, String) {
        (VERSION_KEY, meta::CRATE_VERSION.to_string())
    }
}

const TYPE_KEY: &str = concatcp!(meta::CRATE_NAME, "/type");

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

impl labels::ToLabel for ObjectType {
    fn to_label(&self) -> (&'static str, String) {
        (TYPE_KEY, self.to_string())
    }
}
