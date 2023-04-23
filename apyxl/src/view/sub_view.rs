use crate::model;
use crate::view::{Namespace, Transformer, Transforms};

/// A view into the [Model] starting at a specific [Namespace] with additional [Transforms].
#[derive(Debug)]
pub struct SubView<'a> {
    namespace: &'a model::Namespace<'a>,
    xforms: Transforms,
}

impl<'a> SubView<'a> {
    pub fn new(namespace: &'a model::Namespace<'a>, xforms: Transforms) -> Self {
        Self { namespace, xforms }
    }

    pub fn namespace<'v>(&'v self) -> Namespace<'v, 'a> {
        Namespace::new(self.namespace, &self.xforms)
    }
}

impl Transformer for SubView<'_> {
    fn xforms(&mut self) -> &mut Transforms {
        &mut self.xforms
    }
}

#[cfg(test)]
mod tests {
    use crate::test_util::executor::TestExecutor;
    use crate::view::tests::TestFilter;
    use crate::view::Transformer;
    use itertools::Itertools;

    #[test]
    fn filters() {
        let mut exe = TestExecutor::new(
            r#"
                    mod visible {}
                    mod hidden {}
                    struct visible {}
                    struct hidden {}
                    fn visible() {}
                    fn hidden() {}
                "#,
        );
        let model = exe.model();
        let view = model.view();
        let root = view.api();
        let sub_view = root.sub_view().with_namespace_transform(TestFilter {});
        let namespace = sub_view.namespace();

        assert_eq!(namespace.namespaces().count(), 1);
        assert_eq!(namespace.dtos().count(), 1);
        assert_eq!(namespace.rpcs().count(), 1);

        assert_eq!(
            namespace.namespaces().collect_vec().get(0).unwrap().name(),
            "visible"
        );
        assert_eq!(
            namespace.dtos().collect_vec().get(0).unwrap().name(),
            "visible"
        );
        assert_eq!(
            namespace.rpcs().collect_vec().get(0).unwrap().name(),
            "visible"
        );
    }
}
