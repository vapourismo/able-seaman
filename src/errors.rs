use crate::release::ReleaseError;
use kube::core::DynamicObject;
use std::path::Path;

#[derive(Debug)]
pub enum GeneralError {
    KubeError(kube::error::Error),
    IOError(std::io::Error),
    YAMLError(serde_yaml::Error),
    JSONError(serde_json::Error),
    ObjectWithoutName(DynamicObject),
    DuplicateObject(String),
    FileNotFound(Box<Path>),
    ReleaseError(ReleaseError),
}

impl From<std::io::Error> for GeneralError {
    fn from(error: std::io::Error) -> GeneralError {
        GeneralError::IOError(error)
    }
}

impl From<serde_yaml::Error> for GeneralError {
    fn from(error: serde_yaml::Error) -> GeneralError {
        GeneralError::YAMLError(error)
    }
}

impl From<serde_json::Error> for GeneralError {
    fn from(error: serde_json::Error) -> GeneralError {
        GeneralError::JSONError(error)
    }
}

impl From<kube::error::Error> for GeneralError {
    fn from(error: kube::Error) -> GeneralError {
        GeneralError::KubeError(error)
    }
}

impl From<ReleaseError> for GeneralError {
    fn from(error: ReleaseError) -> GeneralError {
        GeneralError::ReleaseError(error)
    }
}
