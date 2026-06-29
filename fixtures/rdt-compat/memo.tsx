/**
 * Fixture for the React.memo component pattern.
 *
 * React.memo is a HOC that wraps a functional component. The extractor must
 * detect the inner function's prop type even when memo() is the outer binding.
 */
import * as React from 'react';

export interface AvatarProps {
  /** The user's display name. */
  name: string;
  /** URL of the avatar image. */
  src?: string;
  /** Avatar diameter in pixels. @default 40 */
  size?: number;
}

export const Avatar = React.memo(function Avatar(props: AvatarProps) {
  const { name, src, size = 40 } = props;
  return <img src={src} alt={name} width={size} height={size} />;
});
