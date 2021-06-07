use crate::objects::attach_annotations;
use crate::objects::attach_labels;
use kube::core::DynamicObject;

pub struct ReleaseInfo {
    pub name: String,
}

impl ReleaseInfo {
    pub fn configure_object(&self, object: &mut DynamicObject) {
        attach_labels(object, self.name.clone());
        attach_annotations(object);
    }
}
