import { useState, useRef, useEffect } from 'react';
import { ChevronDown, Check, Search } from 'lucide-react';

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
  const [query, setQuery] = useState('');
  const containerRef = useRef<HTMLDivElement>(null);
  const searchRef = useRef<HTMLInputElement>(null);

  // focus the search box when the menu opens; reset the query on close
  useEffect(() => {
    if (isOpen) {
      setQuery('');
      requestAnimationFrame(() => searchRef.current?.focus());
    }
  }, [isOpen]);

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
  const visibleOptions = query
    ? options.filter((o) => o.label.toLowerCase().includes(query.toLowerCase()))
    : options;

  const handleSelect = (val: string) => {
    onValueChange?.(val);
    onChange?.({ target: { value: val } });
    setIsOpen(false);
  };

  const accentStyles = {
    violet: {
      border: 'border-violet-500/60 hover:border-violet-400 focus:border-violet-400 shadow-violet-500/10',
      activeItem: 'bg-violet-500/15 text-violet-700 dark:bg-violet-500/20 dark:text-violet-200',
      badge: 'text-violet-600 dark:text-violet-300',
    },
    sky: {
      border: 'border-sky-500/60 hover:border-sky-400 focus:border-sky-400 shadow-sky-500/10',
      activeItem: 'bg-sky-500/15 text-sky-700 dark:bg-sky-500/20 dark:text-sky-200',
      badge: 'text-sky-600 dark:text-sky-300',
    },
    amber: {
      border: 'border-amber-500/60 hover:border-amber-400 focus:border-amber-400 shadow-amber-500/10',
      activeItem: 'bg-amber-500/15 text-amber-700 dark:bg-amber-500/20 dark:text-amber-200',
      badge: 'text-amber-600 dark:text-amber-300',
    },
    emerald: {
      border: 'border-emerald-500/60 hover:border-emerald-400 focus:border-emerald-400 shadow-emerald-500/10',
      activeItem: 'bg-emerald-500/15 text-emerald-700 dark:bg-emerald-500/20 dark:text-emerald-200',
      badge: 'text-emerald-600 dark:text-emerald-300',
    },
    default: {
      border: 'border-border hover:border-foreground/30 focus:border-ring',
      activeItem: 'bg-muted text-foreground',
      badge: 'text-muted-foreground',
    },
  };

  const currentAccent = accentStyles[accent];
  const hoverItem = {
    violet: 'hover:bg-violet-500/15',
    sky: 'hover:bg-sky-500/15',
    amber: 'hover:bg-amber-500/15',
    emerald: 'hover:bg-emerald-500/15',
    default: 'hover:bg-accent',
  }[accent];

  return (
    <div ref={containerRef} className={`relative w-full nodrag nopan nowheel ${className}`}>
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
        <div className="absolute left-0 top-full z-50 mt-1 max-h-64 w-full min-w-[190px] overflow-y-auto rounded-lg border border-border bg-popover/98 p-1 shadow-2xl backdrop-blur-xl nodrag nopan nowheel">
          {options.length > 4 && (
            <div className="sticky top-0 z-10 mb-1 flex items-center gap-2 rounded-md border border-border/70 bg-popover px-2.5 py-1.5">
              <Search className="h-3 w-3 shrink-0 text-muted-foreground" />
              <input
                ref={searchRef}
                value={query}
                onChange={(e) => setQuery(e.target.value)}
                placeholder="Search…"
                className="w-full bg-transparent text-xs font-medium text-foreground outline-none placeholder:text-muted-foreground nodrag"
              />
            </div>
          )}
          {visibleOptions.length === 0 && (
            <div className="px-2.5 py-2 text-xs text-muted-foreground">No matches</div>
          )}
          {visibleOptions.map((opt) => {
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
                    : `text-popover-foreground ${hoverItem}`
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
