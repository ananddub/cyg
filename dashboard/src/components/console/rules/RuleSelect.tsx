import React from 'react';
import { ChevronDown } from 'lucide-react';

export interface RuleSelectOption {
  value: string;
  label: string;
}

export interface RuleSelectProps extends Omit<React.SelectHTMLAttributes<HTMLSelectElement>, 'size'> {
  options: RuleSelectOption[];
  placeholder?: string;
  accent?: 'violet' | 'sky' | 'amber' | 'emerald' | 'default';
  size?: 'sm' | 'default';
}

export function RuleSelect({
  options,
  placeholder,
  accent = 'default',
  size = 'default',
  className = '',
  value,
  onChange,
  ...props
}: RuleSelectProps) {
  const accentBorders = {
    violet:
      'border-violet-500/50 hover:border-violet-400 focus:border-violet-400 focus:ring-1 focus:ring-violet-400/40 text-violet-100',
    sky:
      'border-sky-500/50 hover:border-sky-400 focus:border-sky-400 focus:ring-1 focus:ring-sky-400/40 text-sky-100',
    amber:
      'border-amber-500/50 hover:border-amber-400 focus:border-amber-400 focus:ring-1 focus:ring-amber-400/40 text-amber-100',
    emerald:
      'border-emerald-500/50 hover:border-emerald-400 focus:border-emerald-400 focus:ring-1 focus:ring-emerald-400/40 text-emerald-100',
    default:
      'border-zinc-700 hover:border-zinc-500 focus:border-zinc-400 focus:ring-1 focus:ring-zinc-400/30 text-zinc-100',
  };

  const heightClasses = size === 'sm' ? 'h-7 py-1 text-[11px]' : 'h-8 py-1.5 text-xs';

  return (
    <div className="relative w-full">
      <select
        value={value}
        onChange={onChange}
        className={`w-full appearance-none rounded-lg bg-zinc-900/95 pl-3 pr-8 font-semibold shadow-md transition-all outline-none cursor-pointer ${heightClasses} ${accentBorders[accent]} ${className}`}
        {...props}
      >
        {placeholder && (
          <option value="" disabled className="bg-zinc-950 font-normal text-zinc-400">
            {placeholder}
          </option>
        )}
        {options.map((opt) => (
          <option
            key={opt.value}
            value={opt.value}
            className="bg-zinc-950 py-2 font-medium text-zinc-100"
          >
            {opt.label}
          </option>
        ))}
      </select>
      <ChevronDown className="pointer-events-none absolute right-2.5 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-zinc-300" />
    </div>
  );
}
