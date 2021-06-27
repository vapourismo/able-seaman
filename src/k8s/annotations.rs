use kube::ResourceExt;
use std::collections::HashMap;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Annotations {
    annotations: HashMap<&'static str, String>,
}

impl Annotations {
    pub fn new() -> Self {
        Annotations {
            annotations: HashMap::new(),
        }
    }

    pub fn set(mut self, name: &'static str, value: String) -> Self {
        self.annotations.insert(name, value);
        self
    }

    pub fn add<L: ToAnnotation>(self, ann: L) -> Self {
        let (name, value) = ann.to_annotation();
        self.set(name, value)
    }

    pub fn apply_to<'a, R: ResourceExt>(&self, subject: &'a mut R) -> &'a mut R {
        subject.annotations_mut().extend(
            self.annotations
                .clone()
                .into_iter()
                .map(|(key, value)| (key.to_string(), value)),
        );

        subject
    }
}

impl Default for Annotations {
    fn default() -> Self {
        Self::new()
    }
}

impl<L: ToAnnotation> From<L> for Annotations {
    fn from(source: L) -> Self {
        Self::new().add(source)
    }
}

pub trait ToAnnotation {
    fn to_annotation(&self) -> (&'static str, String);
}

impl<S: ToString> ToAnnotation for (&'static str, S) {
    fn to_annotation(&self) -> (&'static str, String) {
        (self.0, self.1.to_string())
    }
}

pub trait WithAnnotations {
    fn with_annotations(self, anns: &Annotations) -> Self;

    fn with_annotation<A: ToAnnotation>(self, ann: &A) -> Self;
}

impl<R: ResourceExt> WithAnnotations for R {
    fn with_annotations(mut self, anns: &Annotations) -> Self {
        anns.apply_to(&mut self);
        self
    }

    fn with_annotation<A: ToAnnotation>(mut self, ann: &A) -> Self {
        let (name, value) = ann.to_annotation();
        self.annotations_mut().insert(name.to_string(), value);
        self
    }
}
