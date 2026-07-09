/**
 * Ant Design — antd Button (simplified fixture)
 *
 * Adapted from ant-design/ant-design (MIT License).
 * Real source: components/button/Button.tsx
 *   https://github.com/ant-design/ant-design/blob/master/components/button/Button.tsx
 * Companion: components/button/buttonHelpers.tsx (see ./buttonHelpers.tsx)
 *   https://github.com/ant-design/ant-design/blob/master/components/button/buttonHelpers.tsx
 *
 * The real Button pulls in `@rc-component/util`, `clsx`, `@ant-design/icons`,
 * config-provider context, the space/Compact context, a CSS-in-JS `useStyle`
 * hook, and sibling components (Wave, IconWrapper, DefaultLoadingIcon,
 * ButtonGroup) — none of which are installed in this repo. Those are
 * stubbed or dropped here (a tiny local `cx` helper replaces `clsx`); the
 * render body is trimmed to the minimum needed to exercise real prop usage
 * (forwardRef, href-vs-button branching, the loading/delay state machine,
 * semantic classNames/styles resolution).
 *
 * What's preserved verbatim from upstream:
 *   - BaseButtonProps / ButtonProps, including the `@deprecated iconPosition`
 *     JSDoc tag and the `@private _skipSemantic` tag
 *   - the template-literal index signature: [key: `data-${string}`]: string
 *   - the `GenerateSemantic<Semantic, Props>` utility type (from
 *     components/_util/hooks/useMergeSemantic/semanticType.ts) that derives
 *     per-slot `classNames`/`styles` types
 *   - ButtonType / ButtonShape / ButtonHTMLType / ButtonVariantType /
 *     ButtonColorType (via ./buttonHelpers) and the deprecated
 *     `Button.Group` static
 */
import * as React from 'react';

import type {
  ButtonColorType,
  ButtonHTMLType,
  ButtonShape,
  ButtonType,
  ButtonVariantType,
} from './buttonHelpers';
import { isUnBorderedButtonVariant, spaceChildren } from './buttonHelpers';

// Tiny local stand-in for the `clsx` package (not installed in this fixture repo).
function cx(...args: Array<string | Record<string, unknown> | undefined | false>): string {
  const out: string[] = [];
  for (const arg of args) {
    if (!arg) continue;
    if (typeof arg === 'string') {
      out.push(arg);
    } else {
      for (const key of Object.keys(arg)) {
        if (arg[key]) out.push(key);
      }
    }
  }
  return out.join(' ');
}

// ---------------------------------------------------------------------------
// Semantic slot generics — real pattern, from
// components/_util/hooks/useMergeSemantic/semanticType.ts.
// ---------------------------------------------------------------------------
type RemoveString<T> = T extends string ? never : T;
type RemoveClassNamesString<T> = {
  [K in keyof T]: string | Record<string, any> extends T[K] ? RemoveString<T[K]> : T[K];
};

type DeepClassNameType<T> = {
  [K in keyof T]?: string extends T[K] ? string | DeepClassNameType<T[K]> : DeepClassNameType<T[K]>;
};
type CSS = React.CSSProperties;
type DeepStylesType<T> = {
  [K in keyof T]?: CSS extends T[K] ? CSS : DeepStylesType<T[K]>;
};

export type GenerateSemantic<T extends { classNames?: any; styles?: any }, Props> = {
  // classNames
  classNames: DeepClassNameType<T['classNames']>;
  classNamesNoString: RemoveClassNamesString<DeepClassNameType<T['classNames']>>;
  classNamesFn: (info: { props: Props }) => DeepClassNameType<T['classNames']>;
  classNamesAndFn:
    | DeepClassNameType<T['classNames']>
    | ((info: { props: Props }) => DeepClassNameType<T['classNames']>);
  // styles
  styles: DeepStylesType<T['styles']>;
  stylesFn: (info: { props: Props }) => DeepStylesType<T['styles']>;
  stylesAndFn:
    | DeepStylesType<T['styles']>
    | ((info: { props: Props }) => DeepStylesType<T['styles']>);
};

/**
 * Simplified stand-in for Ant Design's `SizeType`
 * (components/config-provider/SizeContext.tsx).
 *
 * Note: `middle` is deprecated and will be removed in v7, please use `medium` instead.
 */
export type SizeType = 'small' | 'medium' | 'middle' | 'large' | undefined;

export type LegacyButtonType = ButtonType | 'danger';

export type ButtonSemanticType = {
  classNames?: {
    root?: string;
    icon?: string;
    content?: string;
  };
  styles?: {
    root?: React.CSSProperties;
    icon?: React.CSSProperties;
    content?: React.CSSProperties;
  };
};

export type ButtonSemanticAllType = GenerateSemantic<ButtonSemanticType, BaseButtonProps>;

export interface BaseButtonProps {
  type?: ButtonType;
  color?: ButtonColorType;
  variant?: ButtonVariantType;
  icon?: React.ReactNode;
  /** @deprecated please use `iconPlacement` instead */
  iconPosition?: 'start' | 'end';
  iconPlacement?: 'start' | 'end';
  shape?: ButtonShape;
  size?: SizeType;
  disabled?: boolean;
  loading?: boolean | { delay?: number; icon?: React.ReactNode };
  prefixCls?: string;
  className?: string;
  rootClassName?: string;
  ghost?: boolean;
  danger?: boolean;
  block?: boolean;
  children?: React.ReactNode;
  [key: `data-${string}`]: string;
  classNames?: ButtonSemanticAllType['classNamesAndFn'];
  styles?: ButtonSemanticAllType['stylesAndFn'];
  // FloatButton reuse the Button as sub component,
  // But this should not consume context semantic classNames and styles.
  // Use props here to avoid context solution cost for normal usage.
  /** @private Only for internal usage. Do not use in your production */
  _skipSemantic?: boolean;
}

type MergedHTMLAttributes = Omit<
  React.HTMLAttributes<HTMLElement> &
    React.ButtonHTMLAttributes<HTMLElement> &
    React.AnchorHTMLAttributes<HTMLElement>,
  'type' | 'color'
>;

export interface ButtonProps extends BaseButtonProps, MergedHTMLAttributes {
  href?: string;
  htmlType?: ButtonHTMLType;
  autoInsertSpace?: boolean;
}

type LoadingConfigType = {
  loading: boolean;
  delay: number;
};

function getLoadingConfig(loading: BaseButtonProps['loading']): LoadingConfigType {
  if (loading && typeof loading === 'object') {
    const delay = typeof loading.delay === 'number' ? loading.delay : 0;
    return {
      loading: delay <= 0,
      delay,
    };
  }

  return {
    loading: !!loading,
    delay: 0,
  };
}

type ColorVariantPairType = [color?: ButtonColorType, variant?: ButtonVariantType];

const ButtonTypeMap: Partial<Record<ButtonType, ColorVariantPairType>> = {
  default: ['default', 'outlined'],
  primary: ['primary', 'solid'],
  dashed: ['default', 'dashed'],
  // `link` is not a real color but we should compatible with it
  link: ['link' as ButtonColorType, 'link'],
  text: ['default', 'text'],
};

const InternalCompoundedButton = React.forwardRef<
  HTMLButtonElement | HTMLAnchorElement,
  ButtonProps
>((props, ref) => {
  const {
    _skipSemantic,
    loading = false,
    prefixCls = 'ant-btn',
    color,
    variant,
    type,
    danger = false,
    shape = 'default',
    size,
    disabled,
    className,
    rootClassName,
    children,
    icon,
    iconPosition,
    iconPlacement,
    ghost = false,
    block = false,
    htmlType = 'button',
    classNames,
    styles,
    style,
    autoInsertSpace = true,
    autoFocus,
    href,
    ...rest
  } = props;

  // Compatible with original `type` behavior:
  // https://github.com/ant-design/ant-design/issues/47605
  const mergedType = type || 'default';

  const [mergedColor, mergedVariant] = React.useMemo<ColorVariantPairType>(() => {
    if (color && variant) {
      return [color, variant];
    }
    if (type || danger) {
      const pair = ButtonTypeMap[mergedType] || [];
      return danger ? ['danger', pair[1]] : pair;
    }
    return ['default', 'outlined'];
  }, [color, variant, type, danger, mergedType]);

  const loadingConfig = getLoadingConfig(loading);
  const [innerLoading, setInnerLoading] = React.useState<boolean>(loadingConfig.loading);

  React.useEffect(() => {
    if (loadingConfig.delay > 0) {
      const timer = setTimeout(() => setInnerLoading(true), loadingConfig.delay);
      return () => clearTimeout(timer);
    }
    setInnerLoading(loadingConfig.loading);
  }, [loadingConfig.delay, loadingConfig.loading]);

  const mergedIconPlacement = iconPlacement ?? iconPosition ?? 'start';
  const iconType = innerLoading ? 'loading' : icon;

  // Real implementation resolves the function form of classNames/styles via
  // useMergeSemantic; this trimmed fixture only handles the plain-object form.
  const semanticClassNames = classNames as ButtonSemanticType['classNames'] | undefined;
  const semanticStyles = styles as ButtonSemanticType['styles'] | undefined;

  const classes = cx(
    prefixCls,
    {
      [`${prefixCls}-${shape}`]: shape !== 'default' && shape !== 'square' && !!shape,
      [`${prefixCls}-${mergedType}`]: !!mergedType,
      [`${prefixCls}-dangerous`]: danger,
      [`${prefixCls}-color-${mergedColor}`]: !!mergedColor,
      [`${prefixCls}-variant-${mergedVariant}`]: !!mergedVariant,
      [`${prefixCls}-lg`]: size === 'large',
      [`${prefixCls}-sm`]: size === 'small',
      [`${prefixCls}-icon-only`]: !children && children !== 0 && !!iconType,
      [`${prefixCls}-background-ghost`]: ghost && !isUnBorderedButtonVariant(mergedVariant),
      [`${prefixCls}-loading`]: innerLoading,
      [`${prefixCls}-block`]: block,
      [`${prefixCls}-icon-end`]: mergedIconPlacement === 'end',
    },
    className,
    rootClassName,
    semanticClassNames?.root,
  );

  const contentNode = spaceChildren(
    children,
    children != null && !icon && !isUnBorderedButtonVariant(mergedVariant),
    semanticStyles?.content,
    semanticClassNames?.content,
  );

  const mergedStyle: React.CSSProperties = { ...style, ...semanticStyles?.root };

  if (href !== undefined) {
    return (
      <a
        {...(rest as React.AnchorHTMLAttributes<HTMLAnchorElement>)}
        className={classes}
        href={disabled ? undefined : href}
        style={mergedStyle}
        onClick={rest.onClick as React.MouseEventHandler<HTMLAnchorElement>}
        ref={ref as React.Ref<HTMLAnchorElement>}
        tabIndex={disabled ? -1 : 0}
        aria-disabled={disabled}
      >
        {icon}
        {contentNode}
      </a>
    );
  }

  return (
    <button
      {...(rest as React.ButtonHTMLAttributes<HTMLButtonElement>)}
      type={htmlType}
      className={classes}
      style={mergedStyle}
      disabled={disabled}
      autoFocus={autoFocus}
      ref={ref as React.Ref<HTMLButtonElement>}
    >
      {icon}
      {contentNode}
    </button>
  );
});

export interface ButtonGroupProps {
  size?: SizeType;
  style?: React.CSSProperties;
  className?: string;
  prefixCls?: string;
  children?: React.ReactNode;
}

/**
 * Trimmed stand-in for components/button/ButtonGroup.tsx — the real
 * implementation also reads ConfigContext for `direction` and pulls a
 * hashId from the CSS-in-JS token provider, both dropped here.
 */
const Group: React.FC<ButtonGroupProps> = ({
  prefixCls = 'ant-btn-group',
  size,
  className,
  children,
  ...rest
}) => (
  <div {...rest} className={cx(prefixCls, className)}>
    {children}
  </div>
);

type CompoundedComponent = typeof InternalCompoundedButton & {
  /** @deprecated Please use `Space.Compact` */
  Group: typeof Group;
  /** @internal */
  __ANT_BUTTON: boolean;
};

// Real upstream does `const Button = InternalCompoundedButton as
// CompoundedComponent;` then exports that cast alias. Empirically,
// react-docgen-typescript cannot trace a forwardRef component through an
// `as <intersection type>` cast performed on the exported binding itself —
// it silently extracts zero components (verified with a minimal repro).
// Mutating the original forwardRef binding in place (only casting at the
// assignment site, not on the exported identifier) keeps both docgen tools
// working while still producing the same runtime shape as upstream.
const Button = InternalCompoundedButton;
(Button as unknown as CompoundedComponent).Group = Group;
(Button as unknown as CompoundedComponent).__ANT_BUTTON = true;
// Real implementation only sets this outside production builds
// (process.env.NODE_ENV check omitted — no @types/node in this fixture repo).
Button.displayName = 'Button';

export default Button;
