// PropType uses #[serde(tag = "kind")] and nests recursively via ObjectField/Union/etc.
// Each nesting level wraps the serializer in another TaggedSerializer during codegen.
// 2048 is required to compile serde_json::to_string for this type.
#![recursion_limit = "2048"]

pub mod cache;
pub mod extractor;
pub mod import_map;
pub mod known;
pub mod pipeline;
pub mod react_types;
pub mod resolver;
pub mod types;
