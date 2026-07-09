/**
 * Ant Design — antd Button helpers (simplified fixture)
 *
 * Adapted from ant-design/ant-design (MIT License).
 * Real source: components/button/buttonHelpers.tsx
 *   https://github.com/ant-design/ant-design/blob/master/components/button/buttonHelpers.tsx
 *
 * Trimmed: `convertLegacyProps` and the two-Chinese-character spacing logic
 * inside `spaceChildren`/`splitCNCharsBySpace` are dropped — pure rendering
 * behavior, not part of the prop-type contract under test. `PresetColors` is
 * normally imported from components/theme/interface; it's inlined here since
 * the theme package isn't part of this fixture. The five exported union
 * types (ButtonType, ButtonShape, ButtonHTMLType, ButtonVariantType,
 * ButtonColorType) are copied verbatim, as is `isUnBorderedButtonVariant`.
 */
import type * as React from 'react';

// Real Ant Design preset color tokens, normally from
// components/theme/interface/presetColors.ts.
export const PresetColors = [
  'blue',
  'purple',
  'cyan',
  'green',
  'magenta',
  'pink',
  'red',
  'orange',
  'yellow',
  'volcano',
  'geekblue',
  'lime',
  'gold',
] as const;

const rxTwoCNChar = /^[\u4E00-\u9FA5]{2}$/;
export const isTwoCNChar = rxTwoCNChar.test.bind(rxTwoCNChar);

export function isUnBorderedButtonVariant(type?: ButtonVariantType) {
  return type === 'text' || type === 'link';
}

// Trimmed: the real implementation walks `children`, merges adjacent
// two-Chinese-character text nodes with inserted spacing, and clones
// elements to apply className/style. That's pure rendering logic, so this
// fixture keeps only the signature and passes children through untouched.
export function spaceChildren(
  children: React.ReactNode,
  needInserted: boolean,
  style?: React.CSSProperties,
  className?: string,
): React.ReactNode {
  return children;
}

const _ButtonTypes = ['default', 'primary', 'dashed', 'link', 'text'] as const;
export type ButtonType = (typeof _ButtonTypes)[number];

const _ButtonShapes = ['default', 'circle', 'round', 'square'] as const;
export type ButtonShape = (typeof _ButtonShapes)[number];

const _ButtonHTMLTypes = ['submit', 'button', 'reset'] as const;
export type ButtonHTMLType = (typeof _ButtonHTMLTypes)[number];

export const _ButtonVariantTypes = [
  'outlined',
  'dashed',
  'solid',
  'filled',
  'text',
  'link',
] as const;
export type ButtonVariantType = (typeof _ButtonVariantTypes)[number];

export const _ButtonColorTypes = ['default', 'primary', 'danger', ...PresetColors] as const;
export type ButtonColorType = (typeof _ButtonColorTypes)[number];
