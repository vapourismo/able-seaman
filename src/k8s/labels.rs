use kube::ResourceExt;
use std::collections::HashMap;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Labels {
    labels: HashMap<&'static str, String>,
}

impl Labels {
    pub fn new() -> Self {
        Self {
            labels: HashMap::new(),
        }
    }

    pub fn set(mut self, name: &'static str, value: String) -> Self {
        self.labels.insert(name, value);
        self
    }

    pub fn add<L: ToLabel>(self, label: L) -> Self {
        let (name, value) = label.to_label();
        self.set(name, value)
    }

    pub fn to_listparams(&self) -> kube::api::ListParams {
        let it = self
            .labels
            .iter()
            .map(|(name, value)| format!("{}={}", name, value))
            .collect::<Vec<String>>()
            .as_slice()
            .join(",");

        kube::api::ListParams::default().labels(it.as_str())
    }

    pub fn apply_to<'a, R: ResourceExt>(&self, subject: &'a mut R) -> &'a mut R {
        subject.labels_mut().extend(
            self.labels
                .clone()
                .into_iter()
                .map(|(key, value)| (key.to_string(), value)),
        );

        subject
    }
}

impl Default for Labels {
    fn default() -> Self {
        Self::new()
    }
}

impl<L: ToLabel> From<L> for Labels {
    fn from(source: L) -> Self {
        Self::new().add(source)
    }
}

pub trait ToLabel {
    fn to_label(&self) -> (&'static str, String);
}

impl<S: ToString> ToLabel for (&'static str, S) {
    fn to_label(&self) -> (&'static str, String) {
        (self.0, self.1.to_string())
    }
}

impl From<Labels> for kube::api::ListParams {
    fn from(label_selector: Labels) -> Self {
        label_selector.to_listparams()
    }
}

pub trait WithLabels {
    fn with_labels(self, labels: &Labels) -> Self;

    fn with_label<L: ToLabel>(self, label: &L) -> Self;
}

impl<R: ResourceExt> WithLabels for R {
    fn with_labels(mut self, labels: &Labels) -> Self {
        labels.apply_to(&mut self);
        self
    }

    fn with_label<L: ToLabel>(mut self, label: &L) -> Self {
        let (name, value) = label.to_label();
        self.labels_mut().insert(name.to_string(), value);
        self
    }
}
