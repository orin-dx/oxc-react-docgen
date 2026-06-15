// PropType uses #[serde(tag = "kind")] and nests recursively via ObjectField/Union/etc.
// Each nesting level wraps the serializer in another TaggedSerializer during codegen.
// 2048 is required to compile serde_json::to_string for this type.
#![recursion_limit = "2048"]

pub(crate) mod cache;
pub(crate) mod extractor;
pub(crate) mod import_map;
pub(crate) mod known;
pub mod pipeline;
pub mod react_types;
pub(crate) mod resolver;
pub mod types;
