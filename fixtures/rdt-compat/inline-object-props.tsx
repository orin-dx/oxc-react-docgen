import * as React from 'react';

export const Toast = React.forwardRef<HTMLDivElement, { message: string; duration?: number }>(
  ({ message, duration }, ref) => <div ref={ref}>{message}</div>
);
Toast.displayName = 'Toast';

export const Badge: React.FC<{ label: string; variant?: 'info' | 'warning' }> = ({ label, variant }) => (
  <span>{label}</span>
);
