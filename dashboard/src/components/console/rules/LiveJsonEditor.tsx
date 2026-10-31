import { useState, useMemo } from 'react';
import CodeMirror from '@uiw/react-codemirror';
import { json } from '@codemirror/lang-json';
import { Copy, Check, Sparkles, AlertCircle, CheckCircle2 } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { useTheme } from '@/lib/theme';

interface LiveJsonEditorProps {
  value: string;
  onChange: (value: string) => void;
  onApplyToCanvas?: () => void;
}

export function LiveJsonEditor({ value, onChange, onApplyToCanvas }: LiveJsonEditorProps) {
  const [copied, setCopied] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const mode = useTheme();

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

  const handleCopy = async () => {
    try {
      await navigator.clipboard.writeText(value);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch {
      // fallback
    }
  };

  const handleFormat = () => {
    try {
      const parsed = JSON.parse(value);
      const formatted = JSON.stringify(parsed, null, 2);
      onChange(formatted);
    } catch {
      // ignore if invalid
    }
  };

  return (
    <div className="flex h-full flex-col bg-card font-mono text-xs text-foreground">
      {/* Editor Sub-Header Toolbar */}
      <div className="flex items-center justify-between border-b border-border bg-muted/40 px-4 py-2 select-none">
        <div className="flex items-center gap-2.5">
          <span className="font-semibold text-foreground">Live JSON Code Editor</span>
          {isValid ? (
            <span className="flex items-center gap-1.5 rounded-full bg-emerald-500/10 px-2 py-0.5 text-[10px] font-semibold text-emerald-600 dark:text-emerald-400 border border-emerald-500/20">
              <CheckCircle2 className="h-3 w-3 text-emerald-600 dark:text-emerald-400" />
              Valid JSON (Live Synced)
            </span>
          ) : (
            <span
              className="flex items-center gap-1.5 rounded-full bg-rose-500/10 px-2 py-0.5 text-[10px] font-semibold text-rose-600 dark:text-rose-400 border border-rose-500/20 max-w-xs truncate"
              title={error || ''}
            >
              <AlertCircle className="h-3 w-3 text-rose-600 dark:text-rose-400 flex-shrink-0" />
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
            className="h-7 gap-1 px-2 text-[11px] font-medium text-muted-foreground hover:bg-muted hover:text-foreground disabled:opacity-40"
            title="Prettify and format JSON"
          >
            <Sparkles className="h-3.5 w-3.5 text-amber-500 dark:text-amber-400" />
            <span>Format</span>
          </Button>

          <Button
            type="button"
            size="sm"
            variant="ghost"
            onClick={handleCopy}
            className="h-7 gap-1 px-2 text-[11px] font-medium text-muted-foreground hover:bg-muted hover:text-foreground"
          >
            {copied ? (
              <>
                <Check className="h-3.5 w-3.5 text-emerald-600 dark:text-emerald-400" />
                <span className="text-emerald-600 dark:text-emerald-400 font-semibold">Copied!</span>
              </>
            ) : (
              <>
                <Copy className="h-3.5 w-3.5 text-muted-foreground" />
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
              className="h-7 text-xs bg-sky-500 hover:bg-sky-400 dark:bg-sky-400 dark:hover:bg-sky-300 text-white dark:text-zinc-950 font-bold px-2.5 ml-1"
            >
              Update Canvas
            </Button>
          )}
        </div>
      </div>

      {/* CodeMirror Package Integration */}
      <div className="flex-1 overflow-auto text-xs">
        <CodeMirror
          value={value}
          height="435px"
          theme={mode === 'dark' ? 'dark' : 'light'}
          extensions={[json()]}
          onChange={(val) => onChange(val)}
          basicSetup={{
            lineNumbers: true,
            foldGutter: true,
            dropCursor: true,
            allowMultipleSelections: false,
            indentOnInput: true,
            bracketMatching: true,
            closeBrackets: true,
            autocompletion: true,
            highlightActiveLine: true,
          }}
          className="text-xs font-mono"
        />
      </div>

      {/* Error banner at bottom if invalid */}
      {error && (
        <div className="border-t border-rose-500/30 bg-rose-500/10 px-4 py-1.5 text-[11px] text-rose-600 dark:text-rose-300 flex items-center gap-2">
          <AlertCircle className="h-3.5 w-3.5 text-rose-500 flex-shrink-0" />
          <span className="truncate">{error}</span>
        </div>
      )}
    </div>
  );
}
