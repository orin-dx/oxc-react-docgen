//! Baked-in knowledge of React and DOM types.
//! No file I/O — these are compile-time constants derived from @types/react.

/// HTML element names recognized as component-level inheritors.
///
/// Maps `@types/react` HTML attribute type names to their corresponding DOM element names.
pub fn html_element_for(type_name: &str) -> Option<&'static str> {
    match type_name {
        "ButtonHTMLAttributes" => Some("button"),
        "InputHTMLAttributes" => Some("input"),
        "TextareaHTMLAttributes" => Some("textarea"),
        "SelectHTMLAttributes" => Some("select"),
        "AnchorHTMLAttributes" => Some("a"),
        "FormHTMLAttributes" => Some("form"),
        "LabelHTMLAttributes" => Some("label"),
        "ImgHTMLAttributes" => Some("img"),
        "VideoHTMLAttributes" => Some("video"),
        "AudioHTMLAttributes" => Some("audio"),
        "HTMLAttributes" => Some("div"),
        "DOMAttributes" => Some("div"),
        "AriaAttributes" => None,             // not an element, but recognized as built-in
        "SVGAttributes" | "SVGProps" => None, // SVG — no single element to pick
        "HTMLProps" => Some("div"),           // generic HTML props → div
        _ => None,
    }
}

/// Types that are terminal — never need further resolution.
///
/// These are React-specific type names that we recognize as builtins and don't
/// attempt to chase through imports.
///
/// `extra` allows callers (e.g. the resolver) to pass additional builtin names
/// from `PipelineOptions.extra_builtins` without needing to change this file.
pub fn is_react_builtin(name: &str, extra: &rustc_hash::FxHashSet<compact_str::CompactString>) -> bool {
    extra.contains(name)
        || matches!(
            name,
            "ReactNode"
                | "ReactElement"
                | "JSX.Element"
                | "CSSProperties"
                | "CSSObject"
                | "SyntheticEvent"
                | "MouseEvent"
                | "KeyboardEvent"
                | "ChangeEvent"
                | "FocusEvent"
                | "FormEvent"
                | "DragEvent"
                | "TouchEvent"
                | "WheelEvent"
                | "AnimationEvent"
                | "TransitionEvent"
                | "ClipboardEvent"
                | "CompositionEvent"
                | "MouseEventHandler"
                | "KeyboardEventHandler"
                | "ChangeEventHandler"
                | "FocusEventHandler"
                | "FormEventHandler"
                | "DragEventHandler"
                | "TouchEventHandler"
                | "WheelEventHandler"
                | "AnimationEventHandler"
                | "TransitionEventHandler"
                | "ClipboardEventHandler"
                | "CompositionEventHandler"
                | "PointerEventHandler"
                | "ReactEventHandler"
                | "SubmitEventHandler"
                | "InputEventHandler"
                | "ToggleEventHandler"
                | "FC"
                | "FunctionComponent"
                | "VFC"
                | "VoidFunctionComponent"
                | "ComponentType"
                | "PropsWithChildren"
                | "PropsWithRef"
                | "RefObject"
                | "Ref"
                | "ForwardedRef"
                | "MutableRefObject"
                | "RefCallback"
                | "LegacyRef"
                | "Context"
                | "Consumer"
                | "Provider"
                | "ComponentPropsWithoutRef"
                | "ComponentPropsWithRef"
                | "ComponentProps"
                | "ElementRef"
                | "ElementType"
                | "ReactPortal"
                | "ReactFragment"
                | "ReactChild"
                | "ForwardRefExoticComponent"
                | "RefAttributes"
                | "ComponentRef"
                | "JSXElementConstructor"
                | "SVGAttributes"
                | "SVGProps"
                | "HTMLProps"
        )
}

/// Returns the curated list of notable HTML attribute prop names for a given element.
///
/// Used to populate `ComponentEntry.notable_inherited` — the subset of inherited
/// HTML props that are most relevant for documentation and prop tables.
pub fn notable_html_attrs(element: &str) -> &'static [&'static str] {
    match element {
        "button" => &[
            "onClick",
            "onKeyDown",
            "onKeyUp",
            "onFocus",
            "onBlur",
            "disabled",
            "type",
            "form",
            "name",
            "value",
            "tabIndex",
            "aria-label",
            "aria-describedby",
            "aria-expanded",
            "aria-pressed",
            "aria-haspopup",
        ],
        "input" => &[
            "onChange",
            "onInput",
            "onFocus",
            "onBlur",
            "value",
            "defaultValue",
            "placeholder",
            "type",
            "disabled",
            "readOnly",
            "required",
            "name",
            "min",
            "max",
            "pattern",
            "autoComplete",
            "checked",
            "defaultChecked",
        ],
        "a" => &["href", "target", "rel", "download", "onClick"],
        "textarea" => &[
            "onChange",
            "value",
            "defaultValue",
            "placeholder",
            "disabled",
            "readOnly",
            "required",
            "rows",
            "maxLength",
        ],
        "select" => &["onChange", "value", "defaultValue", "disabled", "multiple", "required"],
        "form" => &["onSubmit", "onReset", "action", "method", "encType", "noValidate"],
        "img" => &["src", "alt", "width", "height", "loading", "onLoad", "onError"],
        _ => &["onClick", "onFocus", "onBlur", "className", "style", "id", "tabIndex", "aria-label"],
    }
}

/// React 18 vs 19 behavioral differences for component detection.
#[derive(Debug, Clone)]
pub struct ReactVersion {
    /// React 18: FC implicitly includes `children` prop.
    /// React 19: FC does NOT implicitly include children.
    pub implicit_children: bool,
    /// React 19: ref is a plain prop, not via forwardRef.
    pub ref_as_prop: bool,
}

/// React 18 config: children are implicit, ref requires forwardRef.
pub const REACT_18: ReactVersion = ReactVersion { implicit_children: true, ref_as_prop: false };

/// React 19 config: no implicit children, ref is a regular prop.
pub const REACT_19: ReactVersion = ReactVersion { implicit_children: false, ref_as_prop: true };
