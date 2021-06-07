use kube::core::DynamicObject;
use std::collections::BTreeMap;

fn update_labels(labels: &mut BTreeMap<String, String>, release: String) {
    labels.insert("able-seaman/release".to_string(), release);
}

pub fn attach_labels(object: &mut DynamicObject, release: String) {
    if let Some(labels) = &mut object.metadata.labels {
        update_labels(labels, release);
    } else {
        let mut labels: BTreeMap<String, String> = BTreeMap::new();
        update_labels(&mut labels, release);
        object.metadata.labels = Some(labels);
    }
}

fn update_annotations(anns: &mut BTreeMap<String, String>) {
    anns.insert("able-seaman/version".to_string(), "0".to_string());
}

pub fn attach_annotations(object: &mut DynamicObject) {
    if let Some(annotations) = &mut object.metadata.annotations {
        update_annotations(annotations);
    } else {
        let mut annotations: BTreeMap<String, String> = BTreeMap::new();
        update_annotations(&mut annotations);
        object.metadata.annotations = Some(annotations);
    }
}
