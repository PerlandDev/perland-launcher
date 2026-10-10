import type { ButtonHTMLAttributes, ReactNode, Ref } from 'react';

export interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: 'primary' | 'secondary' | 'danger' | 'ghost';
  size?: 'md' | 'lg' | 'icon';
  icon?: ReactNode;
  /** React 19 passes refs as props; it reaches the <button> through `rest`. */
  ref?: Ref<HTMLButtonElement>;
}

export function Button({ variant = 'secondary', size = 'md', icon, className, children, type = 'button', ...rest }: ButtonProps) {
  const classes = [
    'btn',
    variant !== 'secondary' && `btn-${variant}`,
    size === 'lg' && 'btn-lg',
    size === 'icon' && 'btn-icon',
    className,
  ]
    .filter(Boolean)
    .join(' ');
  return (
    <button type={type} className={classes} {...rest}>
      {icon}
      {children}
    </button>
  );
}
