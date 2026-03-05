import { useLexicalComposerContext } from '@lexical/react/LexicalComposerContext';
import { FORMAT_TEXT_COMMAND, UNDO_COMMAND } from 'lexical';
import {
  Bold,
  Italic,
  Strikethrough,
  Code,
  Undo2,
} from 'lucide-react';
import { cn } from '@/lib/utils';

interface ToolbarButtonProps {
  onClick: () => void;
  icon: React.ReactNode;
  label: string;
  active?: boolean;
}

function ToolbarButton({ onClick, icon, label, active }: ToolbarButtonProps) {
  return (
    <button
      type="button"
      onMouseDown={(e) => {
        e.preventDefault();
        onClick();
      }}
      aria-label={label}
      title={label}
      className={cn(
        'p-1.5 rounded-sm transition-colors',
        active
          ? 'text-foreground bg-accent'
          : 'text-muted-foreground hover:text-foreground hover:bg-accent/50'
      )}
    >
      {icon}
    </button>
  );
}

export function StaticToolbarPlugin() {
  const [editor] = useLexicalComposerContext();
  const iconSize = 16;

  return (
    <div className="flex items-center gap-0.5 pt-2 border-t border-border/50">
      <ToolbarButton
        onClick={() => editor.dispatchCommand(UNDO_COMMAND, undefined)}
        icon={<Undo2 size={iconSize} />}
        label="撤销"
      />

      <div className="w-px h-4 bg-border mx-1" />

      <ToolbarButton
        onClick={() => editor.dispatchCommand(FORMAT_TEXT_COMMAND, 'bold')}
        icon={<Bold size={iconSize} />}
        label="加粗"
      />
      <ToolbarButton
        onClick={() => editor.dispatchCommand(FORMAT_TEXT_COMMAND, 'italic')}
        icon={<Italic size={iconSize} />}
        label="斜体"
      />
      <ToolbarButton
        onClick={() =>
          editor.dispatchCommand(FORMAT_TEXT_COMMAND, 'strikethrough')
        }
        icon={<Strikethrough size={iconSize} />}
        label="删除线"
      />
      <ToolbarButton
        onClick={() => editor.dispatchCommand(FORMAT_TEXT_COMMAND, 'code')}
        icon={<Code size={iconSize} />}
        label="代码"
      />
    </div>
  );
}
