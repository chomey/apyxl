use anyhow::Result;
use chumsky::prelude::*;
use chumsky::text::whitespace;

use crate::model::{Api, Dto, DtoRef, Field, Rpc};
use crate::Input;
use crate::Parser as ApyxlParser;

type Error<'a> = extra::Err<Simple<'a, char>>;

struct Rust {}

impl ApyxlParser for Rust {
    fn parse(&self, input: &dyn Input) -> Result<Api> {
        // parser().parse(input.data().chars())
        Ok(Api::default())
    }
}

fn dto_ref<'a>() -> impl Parser<'a, &'a str, DtoRef, Error<'a>> {
    // todo type can't be ident (e.g. generics vec/map)
    // todo package pathing
    // todo reference one or more other types (and be able to cross ref that in api)
    text::ident().map(|x: &str| DtoRef { name: x.to_owned() })
}

fn field<'a>() -> impl Parser<'a, &'a str, Field, Error<'a>> {
    let ty = dto_ref();
    let field = text::ident()
        .then_ignore(just(':').padded())
        .then(ty)
        .padded();
    field.map(|(name, ty)| Field {
        name: name.to_owned(),
        ty,
    })
}

fn dto<'a>() -> impl Parser<'a, &'a str, Dto, Error<'a>> {
    let fields = field()
        .separated_by(just(',').padded())
        .allow_trailing()
        .collect::<Vec<_>>()
        .delimited_by(just('{').padded(), just('}').padded());
    let name = text::keyword("struct").padded().ignore_then(text::ident());
    let dto = name.then(fields);
    dto.map(|(name, fields)| Dto {
        name: name.to_owned(),
        fields,
    })
}

fn ignore_fn_body<'a>() -> impl Parser<'a, &'a str, (), Error<'a>> {
    let anything = any().repeated().collect::<Vec<_>>();
    recursive(|nested| nested.delimited_by(just('{'), just('}')).or(anything)).ignored()
}

fn rpc<'a>() -> impl Parser<'a, &'a str, Rpc, Error<'a>> {
    let fn_keyword = text::keyword("pub")
        .then(whitespace().at_least(1))
        .or_not()
        .then(text::keyword("fn"));
    let name = fn_keyword.padded().ignore_then(text::ident());
    let params = field()
        .separated_by(just(',').padded())
        .allow_trailing()
        .collect::<Vec<_>>()
        .delimited_by(just('(').padded(), just(')').padded());
    let return_type = just('-')
        .ignore_then(just('>'))
        .ignore_then(whitespace())
        .ignore_then(dto_ref());
    name.then(params)
        .then(return_type.or_not())
        .then_ignore(ignore_fn_body().padded())
        .map(|((name, params), return_type)| Rpc {
            name: name.to_owned(),
            params,
            return_type,
        })
}

// fn api<'a>() -> impl Parser<'a, &'a str, Api> {}

#[cfg(test)]
mod test {
    use crate::parser::rust::{dto, field};
    use chumsky::error::Simple;
    use chumsky::Parser;

    type TestError = Vec<Simple<'static, char>>;

    #[test]
    fn test_field() -> Result<(), TestError> {
        let result = field().parse("name: Type");
        let output = result.into_result()?;
        assert_eq!(output.name, "name");
        assert_eq!(output.ty.name, "Type");
        Ok(())
    }

    #[test]
    fn test_dto() -> Result<(), TestError> {
        let dto = dto()
            .parse(
                r#"
        struct StructName {
            field0: i32,
            field1: f32,
        }
        "#,
            )
            .into_result()?;
        assert_eq!(&dto.name, "StructName");
        assert_eq!(dto.fields.len(), 2);
        assert_eq!(&dto.fields[0].name, "field0");
        assert_eq!(&dto.fields[1].name, "field1");
        Ok(())
    }

    mod rpc {
        use crate::parser::rust::rpc;
        use crate::parser::rust::test::TestError;
        use chumsky::Parser;

        #[test]
        fn empty_fn() -> Result<(), TestError> {
            let rpc = rpc()
                .parse(
                    r#"
            fn rpc_name() {}
            "#,
                )
                .into_result()?;
            assert_eq!(&rpc.name, "rpc_name");
            assert!(rpc.params.is_empty());
            assert!(rpc.return_type.is_none());
            Ok(())
        }

        #[test]
        fn pub_fn() -> Result<(), TestError> {
            let rpc = rpc()
                .parse(
                    r#"
            pub fn rpc_name() {}
            "#,
                )
                .into_result()?;
            assert_eq!(&rpc.name, "rpc_name");
            assert!(rpc.params.is_empty());
            assert!(rpc.return_type.is_none());
            Ok(())
        }

        #[test]
        fn fn_keyword_smushed() {
            let rpc = rpc()
                .parse(
                    r#"
            pubfn rpc_name() {}
            "#,
                )
                .into_result();
            assert!(rpc.is_err());
        }

        #[test]
        fn ignore_fn_body() -> Result<(), TestError> {
            let rpc = rpc()
                .parse(
                    r#"
            fn rpc_name() {


                        1234 !@#$%^&*()_+-= asdf

             }
            "#,
                )
                .into_result()?;
            assert_eq!(&rpc.name, "rpc_name");
            assert!(rpc.params.is_empty());
            assert!(rpc.return_type.is_none());
            Ok(())
        }

        #[test]
        fn ignore_brackets_in_fn_body() -> Result<(), TestError> {
            let rpc = rpc()
                .parse(
                    r#"
            fn rpc_name() {
                {}
                {{}}
                {{
                }}
                {
                    {
                        {{}
                        {}}
                    }
                }
            }
            "#,
                )
                .into_result()?;
            assert_eq!(&rpc.name, "rpc_name");
            assert!(rpc.params.is_empty());
            assert!(rpc.return_type.is_none());
            Ok(())
        }

        #[test]
        fn single_param() -> Result<(), TestError> {
            let rpc = rpc()
                .parse(
                    r#"
            fn rpc_name(param0: ParamType0) {}
            "#,
                )
                .into_result()?;
            assert_eq!(rpc.params.len(), 1);
            assert_eq!(rpc.params[0].name, "param0");
            assert_eq!(rpc.params[0].ty.name, "ParamType0");
            Ok(())
        }

        #[test]
        fn multiple_params() -> Result<(), TestError> {
            let rpc = rpc()
                .parse(
                    r#"
            fn rpc_name(param0: ParamType0, param1: ParamType1, param2: ParamType2) {}
            "#,
                )
                .into_result()?;
            assert_eq!(rpc.params.len(), 3);
            assert_eq!(rpc.params[0].name, "param0");
            assert_eq!(rpc.params[0].ty.name, "ParamType0");
            assert_eq!(rpc.params[1].name, "param1");
            assert_eq!(rpc.params[1].ty.name, "ParamType1");
            assert_eq!(rpc.params[2].name, "param2");
            assert_eq!(rpc.params[2].ty.name, "ParamType2");
            Ok(())
        }

        #[test]
        fn multiple_params_weird_spacing_trailing_comma() -> Result<(), TestError> {
            let rpc = rpc()
                .parse(
                    r#"
            fn rpc_name(param0: ParamType0      , param1
            :    ParamType1     , param2 :ParamType2
                ,
                ) {}
            "#,
                )
                .into_result()?;
            assert_eq!(rpc.params.len(), 3);
            assert_eq!(rpc.params[0].name, "param0");
            assert_eq!(rpc.params[0].ty.name, "ParamType0");
            assert_eq!(rpc.params[1].name, "param1");
            assert_eq!(rpc.params[1].ty.name, "ParamType1");
            assert_eq!(rpc.params[2].name, "param2");
            assert_eq!(rpc.params[2].ty.name, "ParamType2");
            Ok(())
        }

        #[test]
        fn return_type() -> Result<(), TestError> {
            let rpc = rpc()
                .parse(
                    r#"
            fn rpc_name() -> Asdfg {}
            "#,
                )
                .into_result()?;
            assert_eq!(rpc.return_type.map(|x| x.name), Some("Asdfg".to_owned()));
            Ok(())
        }

        #[test]
        fn return_type_weird_spacing() -> Result<(), TestError> {
            let rpc = rpc()
                .parse(
                    r#"
            fn rpc_name()           ->Asdfg{}
            "#,
                )
                .into_result()?;
            assert_eq!(rpc.return_type.map(|x| x.name), Some("Asdfg".to_owned()));
            Ok(())
        }
    }
}
