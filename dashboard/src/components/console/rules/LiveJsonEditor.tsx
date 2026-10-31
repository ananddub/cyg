import { useState, useRef, useMemo, type KeyboardEvent, type ChangeEvent } from 'react';
import { JsonTokens } from '@/components/console/json-view';
import { Copy, Check, Sparkles, AlertCircle, CheckCircle2 } from 'lucide-react';
import { Button } from '@/components/ui/button';

interface LiveJsonEditorProps {
  value: string;
  onChange: (value: string) => void;
  onApplyToCanvas?: () => void;
}

export function LiveJsonEditor({ value, onChange, onApplyToCanvas }: LiveJsonEditorProps) {
  const [copied, setCopied] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const preRef = useRef<HTMLPreElement>(null);
  const lineNumbersRef = useRef<HTMLDivElement>(null);

  // Validate on value change
  const isValid = useMemo(() => {
    try {
      JSON.parse(value);
      setError(null);
      return true;
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Invalid JSON');
      return false;
    }
  }, [value]);

  const lines = useMemo(() => value.split('\n'), [value]);

  // Synchronize scroll between textarea, syntax pre, and line numbers
  const handleScroll = () => {
    if (!textareaRef.current) return;
    const { scrollTop, scrollLeft } = textareaRef.current;
    if (preRef.current) {
      preRef.current.scrollTop = scrollTop;
      preRef.current.scrollLeft = scrollLeft;
    }
    if (lineNumbersRef.current) {
      lineNumbersRef.current.scrollTop = scrollTop;
    }
  };

  const handleKeyDown = (e: KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === 'Tab') {
      e.preventDefault();
      const target = e.currentTarget;
      const start = target.selectionStart;
      const end = target.selectionEnd;

      const newValue = value.substring(0, start) + '  ' + value.substring(end);
      onChange(newValue);

      requestAnimationFrame(() => {
        if (textareaRef.current) {
          textareaRef.current.selectionStart = textareaRef.current.selectionEnd = start + 2;
        }
      });
    }
  };

  const handleCopy = () => {
    navigator.clipboard.writeText(value);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  const handleFormat = () => {
    try {
      const parsed = JSON.parse(value);
      const formatted = JSON.stringify(parsed, null, 2);
      onChange(formatted);
      setError(null);
    } catch {
      // ignore formatting if invalid JSON
    }
  };

  return (
    <div className="flex h-full flex-col bg-zinc-950 font-mono text-xs">
      {/* Editor Sub-Header Toolbar */}
      <div className="flex items-center justify-between border-b border-border/50 bg-zinc-900/60 px-4 py-2 select-none">
        <div className="flex items-center gap-2.5">
          <span className="font-semibold text-zinc-300">Live JSON Code Editor</span>
          {isValid ? (
            <span className="flex items-center gap-1.5 rounded-full bg-emerald-500/10 px-2 py-0.5 text-[10px] font-semibold text-emerald-400 border border-emerald-500/20">
              <CheckCircle2 className="h-3 w-3 text-emerald-400" />
              Valid JSON (Live Synced)
            </span>
          ) : (
            <span
              className="flex items-center gap-1.5 rounded-full bg-rose-500/10 px-2 py-0.5 text-[10px] font-semibold text-rose-400 border border-rose-500/20 max-w-xs truncate"
              title={error || ''}
            >
              <AlertCircle className="h-3 w-3 text-rose-400 flex-shrink-0" />
              <span className="truncate">Syntax Error</span>
            </span>
          )}
        </div>

        <div className="flex items-center gap-2">
          <Button
            type="button"
            size="sm"
            variant="ghost"
            onClick={handleFormat}
            disabled={!isValid}
            className="h-7 gap-1 px-2 text-[11px] font-medium text-zinc-300 hover:bg-zinc-800 hover:text-zinc-100 disabled:opacity-40"
            title="Prettify and format JSON"
          >
            <Sparkles className="h-3.5 w-3.5 text-amber-400" />
            <span>Format</span>
          </Button>

          <Button
            type="button"
            size="sm"
            variant="ghost"
            onClick={handleCopy}
            className="h-7 gap-1 px-2 text-[11px] font-medium text-zinc-300 hover:bg-zinc-800 hover:text-zinc-100"
          >
            {copied ? (
              <>
                <Check className="h-3.5 w-3.5 text-emerald-400" />
                <span className="text-emerald-400 font-semibold">Copied!</span>
              </>
            ) : (
              <>
                <Copy className="h-3.5 w-3.5 text-zinc-400" />
                <span>Copy</span>
              </>
            )}
          </Button>

          {onApplyToCanvas && (
            <Button
              type="button"
              size="sm"
              disabled={!isValid}
              onClick={onApplyToCanvas}
              className="h-7 text-xs bg-sky-500 hover:bg-sky-400 text-zinc-950 font-bold px-2.5 ml-1"
            >
              Update Canvas
            </Button>
          )}
        </div>
      </div>

      {/* Editor Body: Gutter + Syntax Overlay + Input */}
      <div className="relative flex flex-1 overflow-hidden">
        {/* Line Numbers Gutter */}
        <div
          ref={lineNumbersRef}
          className="select-none overflow-hidden bg-zinc-950/80 py-4 pl-3 pr-3 text-right font-mono text-zinc-600 border-r border-zinc-800/80"
          style={{ minWidth: '42px' }}
        >
          {lines.map((_, i) => (
            <div key={i} className="leading-6">
              {i + 1}
            </div>
          ))}
        </div>

        {/* Code Container */}
        <div className="relative flex-1 overflow-hidden">
          {/* Syntax Highlighted View Underneath */}
          <pre
            ref={preRef}
            aria-hidden="true"
            className="absolute inset-0 m-0 overflow-hidden p-4 font-mono text-xs leading-6 text-json-punct whitespace-pre pointer-events-none select-none"
            style={{ tabSize: 2 }}
          >
            <JsonTokens text={value} />
          </pre>

          {/* Transparent Input Textarea on Top */}
          <textarea
            ref={textareaRef}
            value={value}
            onChange={(e: ChangeEvent<HTMLTextAreaElement>) => onChange(e.target.value)}
            onScroll={handleScroll}
            onKeyDown={handleKeyDown}
            spellCheck={false}
            autoCapitalize="off"
            autoComplete="off"
            autoCorrect="off"
            className="absolute inset-0 m-0 h-full w-full resize-none overflow-auto p-4 font-mono text-xs leading-6 bg-transparent text-transparent caret-sky-400 outline-none border-0 focus:ring-0 whitespace-pre selection:bg-sky-500/25"
            style={{
              tabSize: 2,
              WebkitTextFillColor: 'transparent',
            }}
          />
        </div>
      </div>

      {/* Error banner at bottom if invalid */}
      {error && (
        <div className="border-t border-rose-500/30 bg-rose-950/40 px-4 py-1.5 text-[11px] text-rose-300 flex items-center gap-2">
          <AlertCircle className="h-3.5 w-3.5 text-rose-400 flex-shrink-0" />
          <span className="truncate">{error}</span>
        </div>
      )}
    </div>
  );
}
