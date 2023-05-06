use std::borrow::Cow;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};
use chumsky::prelude::*;
use chumsky::text::whitespace;
use log::debug;

use crate::model::{
    Api, Dto, EntityId, Field, Namespace, NamespaceChild, Rpc, Type, UNDEFINED_NAMESPACE,
};
use crate::Parser as ApyxlParser;
use crate::{model, Input};

type Error<'a> = extra::Err<Simple<'a, char>>;

#[derive(Default)]
pub struct Rust {}

impl ApyxlParser for Rust {
    fn parse<'a, I: Input + 'a>(&self, input: &'a mut I) -> Result<model::Builder<'a>> {
        let mut builder = model::Builder::default();

        for (chunk, data) in input.chunks() {
            debug!("parsing chunk {:?}", chunk.relative_file_path);
            if let Some(file_path) = &chunk.relative_file_path {
                for component in path_iter(&namespace_path(file_path)) {
                    builder.enter_namespace(&component)
                }
            }

            let children = choice((use_decl().ignored(), comment().ignored()))
                .padded()
                .repeated()
                .collect::<Vec<_>>()
                .ignore_then(namespace_children(namespace()).padded())
                .then_ignore(end())
                .parse(&data)
                .into_result()
                .map_err(|err| anyhow!("errors encountered while parsing: {:?}", err))?;

            builder.merge_from_chunk(
                Api {
                    name: Cow::Borrowed(UNDEFINED_NAMESPACE),
                    children,
                    attributes: Default::default(),
                },
                chunk,
            );
            builder.clear_namespace();
        }

        Ok(builder)
    }
}

/// Iterate over path as strings.
fn path_iter<'a>(path: &'a Path) -> impl Iterator<Item = Cow<'a, str>> + 'a {
    path.iter().map(|p| p.to_string_lossy())
}

/// Convert file path to rust module path, obeying rules for {lib,mod}.rs.
fn namespace_path(file_path: &Path) -> PathBuf {
    if file_path.ends_with("mod.rs") || file_path.ends_with("lib.rs") {
        file_path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or(PathBuf::default())
    } else {
        file_path.with_extension("")
    }
}

const ALLOWED_TYPE_NAME_CHARS: &str = "_&<>";

fn type_name<'a>() -> impl Parser<'a, &'a str, &'a str, Error<'a>> {
    any()
        // first char
        .filter(|c: &char| c.is_ascii_alphabetic() || ALLOWED_TYPE_NAME_CHARS.contains(*c))
        // remaining chars
        .then(
            any()
                .filter(|c: &char| c.is_ascii_alphanumeric() || *c == '_')
                .repeated(),
        )
        .slice()
}

fn use_decl<'a>() -> impl Parser<'a, &'a str, (), Error<'a>> {
    text::keyword("pub")
        .then(whitespace().at_least(1))
        .or_not()
        .then(text::keyword("use"))
        .then(whitespace().at_least(1))
        .then(text::ident().separated_by(just("::")).at_least(1))
        .then(just(';'))
        .ignored()
}

// Macro that expands `ty` to the type itself _or_ a ref of the type, e.g. u8 or &u8.
// The macro keeps everything as static str.
macro_rules! ty_or_ref {
    ($ty:literal) => {
        just($ty).or(just(concat!('&', $ty)))
    };
}

fn ty<'a>() -> impl Parser<'a, &'a str, Type, Error<'a>> {
    choice((
        ty_or_ref!("u8").map(|_| Type::U8),
        ty_or_ref!("u16").map(|_| Type::U16),
        ty_or_ref!("u32").map(|_| Type::U32),
        ty_or_ref!("u64").map(|_| Type::U64),
        ty_or_ref!("u128").map(|_| Type::U128),
        ty_or_ref!("i8").map(|_| Type::I8),
        ty_or_ref!("i16").map(|_| Type::I16),
        ty_or_ref!("i32").map(|_| Type::I32),
        ty_or_ref!("i64").map(|_| Type::I64),
        ty_or_ref!("i128").map(|_| Type::I128),
        ty_or_ref!("f8").map(|_| Type::F8),
        ty_or_ref!("f16").map(|_| Type::F16),
        ty_or_ref!("f32").map(|_| Type::F32),
        ty_or_ref!("f64").map(|_| Type::F64),
        ty_or_ref!("f128").map(|_| Type::F128),
        ty_or_ref!("String").map(|_| Type::String),
        ty_or_ref!("Vec<u8>").map(|_| Type::Bytes),
        just("&str").map(|_| Type::String),
        just("&[u8]").map(|_| Type::Bytes),
        entity_id().map(Type::Api),
    ))
}

fn entity_id<'a>() -> impl Parser<'a, &'a str, EntityId, Error<'a>> {
    type_name()
        .separated_by(just("::"))
        .at_least(1)
        .collect::<Vec<_>>()
        .map(|components| EntityId {
            path: components
                .into_iter()
                .map(str::to_string)
                .collect::<Vec<String>>(),
        })
}

fn field<'a>() -> impl Parser<'a, &'a str, Field<'a>, Error<'a>> {
    text::ident()
        .then_ignore(just(':').padded())
        .then(ty())
        .padded()
        .map(|(name, ty)| Field {
            name,
            ty,
            attributes: Default::default(),
        })
        .padded_by(multi_comment())
}

fn dto<'a>() -> impl Parser<'a, &'a str, Dto<'a>, Error<'a>> {
    let attr = just("#[")
        .then(any().and_is(just("]").not()).repeated().slice())
        .then(just(']'));
    let fields = field()
        .separated_by(just(',').padded())
        .allow_trailing()
        .collect::<Vec<_>>()
        .padded_by(multi_comment())
        .delimited_by(just('{').padded(), just('}').padded());
    let name = text::keyword("pub")
        .then(whitespace().at_least(1))
        .or_not()
        .ignore_then(text::keyword("struct").padded())
        .ignore_then(text::ident());
    attr.or_not()
        .padded()
        .ignore_then(name)
        .then(fields)
        .map(|(name, fields)| Dto {
            name,
            fields,
            attributes: Default::default(),
        })
}

#[derive(Debug, PartialEq, Eq)]
enum ExprBlock<'a> {
    Comment(&'a str),
    Body(&'a str),
    Nested(Vec<ExprBlock<'a>>),
}

fn block_comment<'a>() -> impl Parser<'a, &'a str, &'a str, Error<'a>> {
    any()
        .and_is(just("*/").not())
        .repeated()
        .slice()
        .map(&str::trim)
        .delimited_by(just("/*"), just("*/"))
}

fn line_comment<'a>() -> impl Parser<'a, &'a str, &'a str, Error<'a>> {
    just("//").ignore_then(
        any()
            .and_is(just('\n').not())
            .repeated()
            .slice()
            .map(&str::trim),
    )
}

fn comment<'a>() -> impl Parser<'a, &'a str, &'a str, Error<'a>> {
    choice((line_comment(), block_comment()))
}

fn multi_comment<'a>() -> impl Parser<'a, &'a str, Vec<&'a str>, Error<'a>> {
    comment().padded().repeated().collect::<Vec<_>>()
}

fn expr_block<'a>() -> impl Parser<'a, &'a str, Vec<ExprBlock<'a>>, Error<'a>> {
    let body = none_of("{}").repeated().at_least(1).slice().map(&str::trim);
    recursive(|nested| {
        choice((
            comment().boxed().padded().map(ExprBlock::Comment),
            nested.map(ExprBlock::Nested),
            body.map(ExprBlock::Body),
        ))
        .repeated()
        .collect::<Vec<_>>()
        .delimited_by(just('{').padded(), just('}').padded())
    })
}

fn rpc<'a>() -> impl Parser<'a, &'a str, Rpc<'a>, Error<'a>> {
    let fn_keyword = text::keyword("pub")
        .then(whitespace().at_least(1))
        .or_not()
        .then(text::keyword("fn"));
    let name = fn_keyword.padded().ignore_then(text::ident());
    let params = field()
        .separated_by(just(',').padded())
        .allow_trailing()
        .collect::<Vec<_>>()
        .padded_by(multi_comment())
        .delimited_by(just('(').padded(), just(')').padded());
    let return_type = just("->").ignore_then(ty().padded());
    name.then(params)
        .then(return_type.or_not())
        .then_ignore(expr_block().padded())
        .map(|((name, params), return_type)| Rpc {
            name,
            params,
            return_type,
            attributes: Default::default(),
        })
}

fn namespace_children<'a>(
    namespace: impl Parser<'a, &'a str, Namespace<'a>, Error<'a>>,
) -> impl Parser<'a, &'a str, Vec<NamespaceChild<'a>>, Error<'a>> {
    choice((
        dto().map(NamespaceChild::Dto),
        rpc().map(NamespaceChild::Rpc),
        namespace.map(NamespaceChild::Namespace),
    ))
    .padded_by(multi_comment())
    .repeated()
    .collect::<Vec<_>>()
}

fn namespace<'a>() -> impl Parser<'a, &'a str, Namespace<'a>, Error<'a>> {
    recursive(|nested| {
        let mod_keyword = text::keyword("pub")
            .then(whitespace().at_least(1))
            .or_not()
            .then(text::keyword("mod"));
        let body = namespace_children(nested)
            .boxed()
            .delimited_by(just('{').padded(), just('}').padded());
        mod_keyword
            .padded()
            .ignore_then(text::ident())
            // or_not to allow declaration-only in the form:
            //      mod name;
            .then(just(';').padded().map(|_| None).or(body.map(|c| Some(c))))
            .map(|(name, children)| Namespace {
                name: Cow::Borrowed(name),
                children: children.unwrap_or(vec![]),
                attributes: Default::default(),
            })
    })
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use chumsky::error::Simple;
    use chumsky::Parser;

    use crate::model::UNDEFINED_NAMESPACE;
    use crate::parser::rust::field;
    use crate::{input, parser, Parser as ApyxlParser};

    type TestError = Vec<Simple<'static, char>>;

    #[test]
    fn test_field() -> Result<(), TestError> {
        let result = field().parse("name: Type");
        let output = result.into_result()?;
        assert_eq!(output.name, "name");
        assert_eq!(output.ty.api().unwrap().name().unwrap(), "Type");
        Ok(())
    }

    #[test]
    fn root_namespace() -> Result<()> {
        let mut input = input::Buffer::new(
            r#"
        // comment
        use asdf;
        // comment
        // comment
        pub use asdf;
        fn rpc() {}
        struct dto {}
        mod namespace {}
        "#,
        );
        let model = parser::Rust::default().parse(&mut input)?.build().unwrap();
        assert_eq!(model.api().name, UNDEFINED_NAMESPACE);
        assert!(model.api().dto("dto").is_some());
        assert!(model.api().rpc("rpc").is_some());
        assert!(model.api().namespace("namespace").is_some());
        Ok(())
    }

    mod file_path_to_mod {
        use crate::model::{Chunk, EntityId};
        use crate::{input, parser, Parser};
        use anyhow::Result;

        #[test]
        fn file_path_including_name_without_ext() -> Result<()> {
            let mut input = input::ChunkBuffer::new();
            input.add_chunk(Chunk::with_relative_file_path("a/b/c.rs"), "struct dto {}");
            let model = parser::Rust::default().parse(&mut input)?.build().unwrap();

            let namespace = model.api().find_namespace(&EntityId::from("a.b.c"));
            assert!(namespace.is_some());
            assert!(namespace.unwrap().dto("dto").is_some());
            Ok(())
        }

        #[test]
        fn ignore_mod_rs() -> Result<()> {
            let mut input = input::ChunkBuffer::new();
            input.add_chunk(
                Chunk::with_relative_file_path("a/b/mod.rs"),
                "struct dto {}",
            );
            let model = parser::Rust::default().parse(&mut input)?.build().unwrap();

            let namespace = model.api().find_namespace(&EntityId::from("a.b"));
            assert!(namespace.is_some());
            assert!(namespace.unwrap().dto("dto").is_some());
            Ok(())
        }

        #[test]
        fn ignore_lib_rs() -> Result<()> {
            let mut input = input::ChunkBuffer::new();
            input.add_chunk(
                Chunk::with_relative_file_path("a/b/lib.rs"),
                "struct dto {}",
            );
            let model = parser::Rust::default().parse(&mut input)?.build().unwrap();

            let namespace = model.api().find_namespace(&EntityId::from("a.b"));
            assert!(namespace.is_some());
            assert!(namespace.unwrap().dto("dto").is_some());
            Ok(())
        }
    }

    mod ty {
        use crate::model::Type;
        use chumsky::Parser;

        use crate::model::EntityId;
        use crate::parser::rust::tests::TestError;
        use crate::parser::rust::ty;

        macro_rules! test {
            ($name: ident, $data:literal, $expected:expr) => {
                #[test]
                fn $name() -> Result<(), TestError> {
                    run_test($data, $expected)
                }
            };
        }

        test!(u8, "u8", Type::U8);
        test!(u16, "u16", Type::U16);
        test!(u32, "u32", Type::U32);
        test!(u64, "u64", Type::U64);
        test!(u128, "u128", Type::U128);
        test!(i8, "i8", Type::I8);
        test!(i16, "i16", Type::I16);
        test!(i32, "i32", Type::I32);
        test!(i64, "i64", Type::I64);
        test!(i128, "i128", Type::I128);
        test!(f8, "f8", Type::F8);
        test!(f16, "f16", Type::F16);
        test!(f32, "f32", Type::F32);
        test!(f64, "f64", Type::F64);
        test!(f128, "f128", Type::F128);
        test!(string, "String", Type::String);
        test!(bytes, "Vec<u8>", Type::Bytes);

        test!(u8_ref, "&u8", Type::U8);
        test!(u16_ref, "&u16", Type::U16);
        test!(u32_ref, "&u32", Type::U32);
        test!(u64_ref, "&u64", Type::U64);
        test!(u128_ref, "&u128", Type::U128);
        test!(i8_ref, "&i8", Type::I8);
        test!(i16_ref, "&i16", Type::I16);
        test!(i32_ref, "&i32", Type::I32);
        test!(i64_ref, "&i64", Type::I64);
        test!(i128_ref, "&i128", Type::I128);
        test!(f8_ref, "&f8", Type::F8);
        test!(f16_ref, "&f16", Type::F16);
        test!(f32_ref, "&f32", Type::F32);
        test!(f64_ref, "&f64", Type::F64);
        test!(f128_ref, "&f128", Type::F128);
        test!(string_ref, "&String", Type::String);
        test!(bytes_ref, "&Vec<u8>", Type::Bytes);

        test!(str, "&str", Type::String);
        test!(bytes_slice, "&[u8]", Type::Bytes);
        test!(entity_id, "a::b::c", Type::Api(EntityId::from("a.b.c")));

        fn run_test(data: &'static str, expected: Type) -> Result<(), TestError> {
            let ty = ty().parse(data).into_result()?;
            assert_eq!(ty, expected);
            Ok(())
        }
    }

    mod entity_id {
        use chumsky::Parser;

        use crate::parser::rust::entity_id;
        use crate::parser::rust::tests::TestError;

        #[test]
        fn starts_with_underscore() -> Result<(), TestError> {
            let id = entity_id().parse("_type").into_result()?;
            assert_eq!(id.path, vec!["_type"]);
            Ok(())
        }

        #[test]
        fn with_path() -> Result<(), TestError> {
            let id = entity_id().parse("a::b::c").into_result()?;
            assert_eq!(id.path, vec!["a", "b", "c"]);
            Ok(())
        }

        #[test]
        fn reference() -> Result<(), TestError> {
            let id = entity_id().parse("&Type").into_result()?;
            assert_eq!(id.path, vec!["&Type"]);
            Ok(())
        }
    }

    mod namespace {
        use chumsky::Parser;

        use crate::model::NamespaceChild;
        use crate::parser::rust::namespace;
        use crate::parser::rust::tests::TestError;

        #[test]
        fn declaration() -> Result<(), TestError> {
            let namespace = namespace()
                .parse(
                    r#"
            mod empty;
            "#,
                )
                .into_result()?;
            assert_eq!(namespace.name, "empty");
            assert!(namespace.children.is_empty());
            Ok(())
        }

        #[test]
        fn empty() -> Result<(), TestError> {
            let namespace = namespace()
                .parse(
                    r#"
            mod empty {}
            "#,
                )
                .into_result()?;
            assert_eq!(namespace.name, "empty");
            assert!(namespace.children.is_empty());
            Ok(())
        }

        #[test]
        fn with_dto() -> Result<(), TestError> {
            let namespace = namespace()
                .parse(
                    r#"
            mod ns {
                struct DtoName {}
            }
            "#,
                )
                .into_result()?;
            assert_eq!(namespace.name, "ns");
            assert_eq!(namespace.children.len(), 1);
            match &namespace.children[0] {
                NamespaceChild::Dto(dto) => assert_eq!(dto.name, "DtoName"),
                _ => panic!("wrong child type"),
            }
            Ok(())
        }

        #[test]
        fn nested() -> Result<(), TestError> {
            let namespace = namespace()
                .parse(
                    r#"
            mod ns0 {
                mod ns1 {}
            }
            "#,
                )
                .into_result()?;
            assert_eq!(namespace.name, "ns0");
            assert_eq!(namespace.children.len(), 1);
            match &namespace.children[0] {
                NamespaceChild::Namespace(ns) => assert_eq!(ns.name, "ns1"),
                _ => panic!("wrong child type"),
            }
            Ok(())
        }

        #[test]
        fn nested_dto() -> Result<(), TestError> {
            let namespace = namespace()
                .parse(
                    r#"
            mod ns0 {
                mod ns1 {
                    struct DtoName {}
                }
            }
            "#,
                )
                .into_result()?;
            assert_eq!(namespace.name, "ns0");
            assert_eq!(namespace.children.len(), 1);
            match &namespace.children[0] {
                NamespaceChild::Namespace(ns) => {
                    assert_eq!(ns.name, "ns1");
                    assert_eq!(ns.children.len(), 1);
                    match &ns.children[0] {
                        NamespaceChild::Dto(dto) => assert_eq!(dto.name, "DtoName"),
                        _ => panic!("ns1: wrong child type"),
                    }
                }
                _ => panic!("ns0: wrong child type"),
            }
            Ok(())
        }
    }

    mod dto {
        use chumsky::Parser;

        use crate::parser::rust::dto;
        use crate::parser::rust::tests::TestError;

        #[test]
        fn empty() -> Result<(), TestError> {
            let dto = dto()
                .parse(
                    r#"
            struct StructName {}
            "#,
                )
                .into_result()?;
            assert_eq!(dto.name, "StructName");
            assert_eq!(dto.fields.len(), 0);
            Ok(())
        }

        #[test]
        fn pub_struct() -> Result<(), TestError> {
            let dto = dto()
                .parse(
                    r#"
            pub struct StructName {}
            "#,
                )
                .into_result()?;
            assert_eq!(dto.name, "StructName");
            assert_eq!(dto.fields.len(), 0);
            Ok(())
        }

        #[test]
        fn ignore_derive() -> Result<(), TestError> {
            let dto = dto()
                .parse(
                    r#"
            #[derive(Whatever)]
            struct StructName {}
            "#,
                )
                .into_result()?;
            assert_eq!(dto.name, "StructName");
            assert_eq!(dto.fields.len(), 0);
            Ok(())
        }

        #[test]
        fn multiple_fields() -> Result<(), TestError> {
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
            assert_eq!(dto.name, "StructName");
            assert_eq!(dto.fields.len(), 2);
            assert_eq!(dto.fields[0].name, "field0");
            assert_eq!(dto.fields[1].name, "field1");
            Ok(())
        }

        #[test]
        fn fields_with_comments() -> Result<(), TestError> {
            let dto = dto()
                .parse(
                    r#"
            struct StructName {
                // asdf
                // asdf
                field0: i32, /* asdf */ field1: f32,
                // asdf
            }
            "#,
                )
                .into_result()?;
            assert_eq!(dto.name, "StructName");
            assert_eq!(dto.fields.len(), 2);
            assert_eq!(dto.fields[0].name, "field0");
            assert_eq!(dto.fields[1].name, "field1");
            Ok(())
        }
    }

    mod rpc {
        use chumsky::Parser;

        use crate::parser::rust::rpc;
        use crate::parser::rust::tests::TestError;

        #[test]
        fn empty_fn() -> Result<(), TestError> {
            let rpc = rpc()
                .parse(
                    r#"
            fn rpc_name() {}
            "#,
                )
                .into_result()?;
            assert_eq!(rpc.name, "rpc_name");
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
            assert_eq!(rpc.name, "rpc_name");
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
            assert_eq!(rpc.params[0].ty.api().unwrap().name(), Some("ParamType0"));
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
            assert_eq!(rpc.params[0].ty.api().unwrap().name(), Some("ParamType0"));
            assert_eq!(rpc.params[1].name, "param1");
            assert_eq!(rpc.params[1].ty.api().unwrap().name(), Some("ParamType1"));
            assert_eq!(rpc.params[2].name, "param2");
            assert_eq!(rpc.params[2].ty.api().unwrap().name(), Some("ParamType2"));
            Ok(())
        }

        #[test]
        fn multiple_params_with_comments() -> Result<(), TestError> {
            let rpc = rpc()
                .parse(
                    r#"
            fn rpc_name(
                // asdf
                // asdf
                param0: ParamType0, /* asdf */ param1: ParamType1,
                // asdf
                param2: ParamType2 /* asdf */
                // asdf
            ) {}
            "#,
                )
                .into_result()?;
            assert_eq!(rpc.params.len(), 3);
            assert_eq!(rpc.params[0].name, "param0");
            assert_eq!(rpc.params[0].ty.api().unwrap().name(), Some("ParamType0"));
            assert_eq!(rpc.params[1].name, "param1");
            assert_eq!(rpc.params[1].ty.api().unwrap().name(), Some("ParamType1"));
            assert_eq!(rpc.params[2].name, "param2");
            assert_eq!(rpc.params[2].ty.api().unwrap().name(), Some("ParamType2"));
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
            assert_eq!(rpc.params[0].ty.api().unwrap().name(), Some("ParamType0"));
            assert_eq!(rpc.params[1].name, "param1");
            assert_eq!(rpc.params[1].ty.api().unwrap().name(), Some("ParamType1"));
            assert_eq!(rpc.params[2].name, "param2");
            assert_eq!(rpc.params[2].ty.api().unwrap().name(), Some("ParamType2"));
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
            assert_eq!(
                rpc.return_type.as_ref().map(|x| x.api().unwrap().name()),
                Some(Some("Asdfg"))
            );
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
            assert_eq!(
                rpc.return_type.as_ref().map(|x| x.api().unwrap().name()),
                Some(Some("Asdfg"))
            );
            Ok(())
        }
    }

    mod comments {
        use chumsky::Parser;

        use crate::parser::rust::tests::TestError;
        use crate::parser::rust::{comment, namespace};

        #[test]
        fn line_comment() -> Result<(), TestError> {
            let value = comment().parse("// line comment").into_result()?;
            assert_eq!(value, "line comment");
            Ok(())
        }

        #[test]
        fn block_comment() -> Result<(), TestError> {
            let value = comment().parse("/* block comment */").into_result()?;
            assert_eq!(value, "block comment");
            Ok(())
        }

        #[test]
        fn line_comment_inside_namespace() -> Result<(), TestError> {
            namespace()
                .parse(
                    r#"
                    mod ns { // comment
                        // comment
                        // comment
                        struct dto {} // comment
                        // comment
                    }
                    "#,
                )
                .into_result()?;
            Ok(())
        }

        #[test]
        fn block_comment_inside_namespace() -> Result<(), TestError> {
            namespace()
                .parse(
                    r#"
                    mod ns { /* comment */
                        /* comment */
                        /* comment */
                        struct dto {} /* comment */
                        /* comment */
                    }
                    "#,
                )
                .into_result()?;
            Ok(())
        }
    }

    mod expr_block {
        use chumsky::{text, Parser};

        use crate::parser::rust::{expr_block, ExprBlock};

        #[test]
        fn complex() {
            let result = expr_block()
                .parse("{left{inner1_left{inner1}inner1_right}middle{inner2}{inner3}right}")
                .into_result();
            assert_eq!(
                result.unwrap(),
                vec![
                    ExprBlock::Body("left"),
                    ExprBlock::Nested(vec![
                        ExprBlock::Body("inner1_left"),
                        ExprBlock::Nested(vec![ExprBlock::Body("inner1"),]),
                        ExprBlock::Body("inner1_right"),
                    ]),
                    ExprBlock::Body("middle"),
                    ExprBlock::Nested(vec![ExprBlock::Body("inner2"),]),
                    ExprBlock::Nested(vec![ExprBlock::Body("inner3"),]),
                    ExprBlock::Body("right"),
                ]
            );
        }

        #[test]
        fn empty() {
            let result = expr_block().parse("{}").into_result();
            assert_eq!(result.unwrap(), vec![]);
        }

        #[test]
        fn arbitrary_content() {
            let result = expr_block()
                .parse(
                    r#"{
                1234 !@#$%^&*()_+-= asdf
            }"#,
                )
                .into_result();
            assert_eq!(
                result.unwrap(),
                vec![ExprBlock::Body("1234 !@#$%^&*()_+-= asdf")]
            );
        }

        #[test]
        fn line_comment() {
            let result = expr_block()
                .parse(
                    r#"
                    { // don't break! }
                    }"#,
                )
                .into_result();
            assert_eq!(result.unwrap(), vec![ExprBlock::Comment("don't break! }")]);
        }

        #[test]
        fn block_comment() {
            let result = expr_block()
                .parse(
                    r#"{
                    { /* don't break! {{{ */ }
                    }"#,
                )
                .into_result();
            assert_eq!(
                result.unwrap(),
                vec![ExprBlock::Nested(vec![ExprBlock::Comment(
                    "don't break! {{{"
                )]),]
            );
        }

        #[test]
        fn continues_parsing_after() {
            let result = expr_block()
                .padded()
                .ignore_then(text::ident().padded())
                .parse(
                    r#"
                {
                  ignored stuff
                }
                not_ignored
                "#,
                )
                .into_result();
            assert!(result.is_ok(), "parse should not fail");
            assert_eq!(result.unwrap(), "not_ignored");
        }
    }
}
