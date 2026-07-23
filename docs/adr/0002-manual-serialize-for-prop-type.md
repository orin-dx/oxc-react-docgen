# 0002. Hand-write Serialize/Deserialize for PropType instead of deriving it

**Status:** Accepted
**Date:** 2026-07-22 (retroactive)

## Context

`PropType` and `CollectedType` are recursive enums — variants nest through
`Vec`/`Box`/`Option` back into the same type (`Union`, `Object`, `Array`,
...). At some point this project derived `Serialize`/`Deserialize` directly
on a tagged `PropType` (`#[serde(tag = "kind")]`) and had to raise
`#![recursion_limit]` to 2048 to get it to compile — each nesting level wraps
the generated serializer in another layer during codegen, and the default
limit (128) isn't enough for a real component tree.

## Decision

`PropType`, `CollectedType`, and `OpaqueReason` don't derive
`Serialize`/`Deserialize`. Each has a hand-written impl (`to_json_value`/
`from_json_value` for the first two) that builds a `serde_json::Value`
directly instead of going through serde's derive machinery.

## Consequences

- Adding a new variant means updating the manual match in both directions by
  hand — the compiler won't remind you the way a derive would.
  `resolver/mod.rs`'s "no wildcard matches" convention is what catches a
  missed arm here instead.
- `CollectedObjectField` (a struct nested inside `CollectedType::Object`, not
  an enum) still derives normally — the recursion problem is specific to the
  recursive *enums*, not every type that touches them.
- The `#![recursion_limit = "2048"]` attribute in `crates/core/src/lib.rs` is
  left over from the old derive-based design. Verified 2026-07-22: the full
  workspace builds and every test passes with it removed — nothing in the
  current manual-impl code path needs it. Worth deleting as a follow-up;
  left as-is here since confirming that wasn't the point of this decision.

## Alternatives considered

`serde(remote)`, or just keeping the recursion-limit bump — both keep the
derive, so you inherit the compile-time cost and a cryptic "recursion limit
exceeded" the moment someone adds one more level of nesting. Manual impls
trade a bit of hand-maintenance for a predictable compile time.
