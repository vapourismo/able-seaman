use kube::core::DynamicObject;

#[derive(Debug)]
pub enum GeneralError {
    IOError(std::io::Error),
    YAMLError(serde_yaml::Error),
    JSONError(serde_json::Error),
    ObjectWithoutName(DynamicObject),
    DuplicateObject(String),
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
