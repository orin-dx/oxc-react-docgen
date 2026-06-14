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
        "AriaAttributes" => None, // not an element, but recognized as built-in
        _ => None,
    }
}

/// Types that are terminal — never need further resolution.
///
/// These are React-specific type names that we recognize as builtins and don't
/// attempt to chase through imports.
pub fn is_react_builtin(name: &str) -> bool {
    matches!(
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
    )
}

/// React 18 vs 19 behavioral differences for component detection.
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
