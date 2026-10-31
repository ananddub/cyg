import { useState, useRef, useEffect } from 'react';
import { ChevronDown, Check } from 'lucide-react';

export interface RuleSelectOption {
  value: string;
  label: string;
}

export interface RuleSelectProps {
  options: RuleSelectOption[];
  value?: string;
  onChange?: (e: { target: { value: string } }) => void;
  onValueChange?: (value: string) => void;
  placeholder?: string;
  accent?: 'violet' | 'sky' | 'amber' | 'emerald' | 'default';
  className?: string;
}

export function RuleSelect({
  options,
  value,
  onChange,
  onValueChange,
  placeholder = 'Select option...',
  accent = 'default',
  className = '',
}: RuleSelectProps) {
  const [isOpen, setIsOpen] = useState(false);
  const containerRef = useRef<HTMLDivElement>(null);

  // Close when clicking outside
  useEffect(() => {
    function handleClickOutside(event: MouseEvent) {
      if (containerRef.current && !containerRef.current.contains(event.target as Node)) {
        setIsOpen(false);
      }
    }
    if (isOpen) {
      document.addEventListener('mousedown', handleClickOutside);
    }
    return () => {
      document.removeEventListener('mousedown', handleClickOutside);
    };
  }, [isOpen]);

  const selectedOption = options.find((opt) => opt.value === value);

  const handleSelect = (val: string) => {
    onValueChange?.(val);
    onChange?.({ target: { value: val } });
    setIsOpen(false);
  };

  const accentStyles = {
    violet: {
      border: 'border-violet-500/60 hover:border-violet-400 focus:border-violet-400 shadow-violet-500/10',
      activeItem: 'bg-violet-500/20 text-violet-200',
      badge: 'text-violet-300',
    },
    sky: {
      border: 'border-sky-500/60 hover:border-sky-400 focus:border-sky-400 shadow-sky-500/10',
      activeItem: 'bg-sky-500/20 text-sky-200',
      badge: 'text-sky-300',
    },
    amber: {
      border: 'border-amber-500/60 hover:border-amber-400 focus:border-amber-400 shadow-amber-500/10',
      activeItem: 'bg-amber-500/20 text-amber-200',
      badge: 'text-amber-300',
    },
    emerald: {
      border: 'border-emerald-500/60 hover:border-emerald-400 focus:border-emerald-400 shadow-emerald-500/10',
      activeItem: 'bg-emerald-500/20 text-emerald-200',
      badge: 'text-emerald-300',
    },
    default: {
      border: 'border-border hover:border-foreground/30 focus:border-ring',
      activeItem: 'bg-muted text-foreground',
      badge: 'text-muted-foreground',
    },
  };

  const currentAccent = accentStyles[accent];

  return (
    <div ref={containerRef} className={`relative w-full nodrag nopan ${className}`}>
      {/* Trigger Button */}
      <button
        type="button"
        data-slot="rule-select-trigger"
        onClick={() => setIsOpen((prev) => !prev)}
        className={`flex h-8 w-full items-center justify-between rounded-lg border bg-card/95 px-2.5 text-xs font-semibold shadow-xs transition-all outline-none cursor-pointer nodrag ${
          currentAccent.border
        } ${isOpen ? 'ring-1 ring-primary/40' : ''}`}
      >
        <span className={selectedOption ? 'text-foreground truncate' : 'text-muted-foreground font-medium truncate'}>
          {selectedOption ? selectedOption.label : placeholder}
        </span>
        <ChevronDown
          className={`h-3.5 w-3.5 shrink-0 text-muted-foreground transition-transform duration-150 ${
            isOpen ? 'rotate-180 text-foreground' : ''
          }`}
        />
      </button>

      {/* Floating Menu Popover */}
      {isOpen && (
        <div className="absolute left-0 top-full z-50 mt-1 max-h-56 w-full min-w-[170px] overflow-y-auto rounded-lg border border-border bg-popover/98 p-1 shadow-2xl backdrop-blur-xl nodrag nopan">
          {options.map((opt) => {
            const isSelected = opt.value === value;
            return (
              <button
                key={opt.value}
                type="button"
                data-slot="rule-select-option"
                data-value={opt.value}
                onClick={() => handleSelect(opt.value)}
                className={`flex w-full items-center justify-between rounded-md px-2.5 py-1.5 text-left text-xs font-semibold transition cursor-pointer nodrag ${
                  isSelected
                    ? `${currentAccent.activeItem} font-bold`
                    : 'text-popover-foreground hover:bg-accent hover:text-accent-foreground'
                }`}
              >
                <span className="truncate">{opt.label}</span>
                {isSelected && <Check className="h-3.5 w-3.5 shrink-0 text-primary" />}
              </button>
            );
          })}
        </div>
      )}
    </div>
  );
}
