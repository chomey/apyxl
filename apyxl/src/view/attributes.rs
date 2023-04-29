use crate::model;
use dyn_clone::DynClone;
use std::fmt::Debug;

#[derive(Debug, Copy, Clone)]
pub struct Attributes<'v> {
    target: &'v model::Attributes,
    xforms: &'v Vec<Box<dyn AttributeTransform>>,
}

impl<'v> Attributes<'v> {
    pub fn new(
        target: &'v model::Attributes,
        xforms: &'v Vec<Box<dyn AttributeTransform>>,
    ) -> Self {
        Self { target, xforms }
    }
}

pub trait AttributeTransform: Debug + DynClone {
    // todo
}

dyn_clone::clone_trait_object!(AttributeTransform);

// #[cfg(test)]
// mod tests {
//     #[test]
//     fn asdf() {
//         todo!()
//     }
// }
