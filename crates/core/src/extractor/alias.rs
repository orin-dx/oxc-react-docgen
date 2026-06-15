//! Type alias classification: Omit, Pick, Partial, Required, Union, Intersection, Passthrough.

use oxc_ast::ast::*;

use crate::types::{CollectedType, CollectedTypeAlias};

use super::SourceDataCollector;

impl<'src> SourceDataCollector<'src> {
    // ─── TypeAlias classification ─────────────────────────────────────────────

    pub(super) fn classify_type_alias<'a>(
        &self,
        _name: &str,
        ty: &TSType<'a>,
    ) -> Option<CollectedTypeAlias> {
        let fp = self.file_path.clone();
        match ty {
            TSType::TSTypeReference(tr) => {
                let ref_name = self.extract_type_ref_name(tr);
                match ref_name.as_str() {
                    "Omit" => {
                        let tp = tr.type_arguments.as_ref()?;
                        if tp.params.len() < 2 {
                            return None;
                        }
                        let (base_name, base_args) =
                            self.extract_type_name_from_type(&tp.params[0])?;
                        let base = CollectedType::Named {
                            name: base_name,
                            args: base_args
                                .into_iter()
                                .map(CollectedType::Raw)
                                .collect(),
                        };
                        let omitted_keys = self.collect_string_union_keys(&tp.params[1]);
                        Some(CollectedTypeAlias::Omit { base, omitted_keys, file_path: fp })
                    }
                    "Pick" => {
                        let tp = tr.type_arguments.as_ref()?;
                        if tp.params.len() < 2 {
                            return None;
                        }
                        let (base_name, base_args) =
                            self.extract_type_name_from_type(&tp.params[0])?;
                        let base = CollectedType::Named {
                            name: base_name,
                            args: base_args
                                .into_iter()
                                .map(CollectedType::Raw)
                                .collect(),
                        };
                        let picked_keys = self.collect_string_union_keys(&tp.params[1]);
                        Some(CollectedTypeAlias::Pick { base, picked_keys, file_path: fp })
                    }
                    "Partial" => {
                        let tp = tr.type_arguments.as_ref()?;
                        let (base_name, base_args) =
                            self.extract_type_name_from_type(tp.params.first()?)?;
                        let base = CollectedType::Named {
                            name: base_name,
                            args: base_args
                                .into_iter()
                                .map(CollectedType::Raw)
                                .collect(),
                        };
                        Some(CollectedTypeAlias::Partial { base, file_path: fp })
                    }
                    "Required" => {
                        let tp = tr.type_arguments.as_ref()?;
                        let (base_name, base_args) =
                            self.extract_type_name_from_type(tp.params.first()?)?;
                        let base = CollectedType::Named {
                            name: base_name,
                            args: base_args
                                .into_iter()
                                .map(CollectedType::Raw)
                                .collect(),
                        };
                        Some(CollectedTypeAlias::Required { base, file_path: fp })
                    }
                    _ => {
                        // Simple passthrough: `type Size = SomeOtherType`
                        let args = self.extract_type_args(&tr.type_arguments);
                        let target = CollectedType::Named {
                            name: ref_name.into(),
                            args: args.into_iter().map(CollectedType::Raw).collect(),
                        };
                        Some(CollectedTypeAlias::Passthrough { target, file_path: fp })
                    }
                }
            }
            TSType::TSUnionType(u) => {
                // Check if all members are string/number literals → LiteralUnion
                let all_string_literals = u.types.iter().all(|t| match t {
                    TSType::TSLiteralType(lit) => {
                        matches!(lit.literal, TSLiteral::StringLiteral(_))
                    }
                    TSType::TSUndefinedKeyword(_) | TSType::TSNullKeyword(_) => true,
                    _ => false,
                });

                let members: Vec<CollectedType> =
                    u.types.iter().map(|t| self.ts_type_to_collected(t)).collect();

                if all_string_literals {
                    let member_strs: Vec<String> =
                        members.iter().map(|m| m.to_raw_string()).collect();
                    return Some(CollectedTypeAlias::LiteralUnion {
                        members: member_strs,
                        file_path: fp,
                    });
                }
                Some(CollectedTypeAlias::Union { members, file_path: fp })
            }
            TSType::TSIntersectionType(i) => {
                let members =
                    i.types.iter().map(|t| self.ts_type_to_collected(t)).collect();
                Some(CollectedTypeAlias::Intersection { members, file_path: fp })
            }
            TSType::TSParenthesizedType(p) => {
                self.classify_type_alias(_name, &p.type_annotation)
            }
            _ => None,
        }
    }

    /// Collect the string literal keys from a type like `'key1' | 'key2'`.
    pub(super) fn collect_string_union_keys<'a>(&self, ty: &TSType<'a>) -> Vec<String> {
        match ty {
            TSType::TSLiteralType(lit) => match &lit.literal {
                TSLiteral::StringLiteral(s) => vec![s.value.as_str().to_owned()],
                _ => vec![],
            },
            TSType::TSUnionType(u) => {
                u.types.iter().flat_map(|t| self.collect_string_union_keys(t)).collect()
            }
            _ => vec![],
        }
    }
}
