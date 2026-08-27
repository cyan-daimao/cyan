/** unified diff 渲染：新增绿 / 删除红 / hunk 蓝 */
export function DiffView({ diff }: { diff: string }) {
  const lines = diff.split('\n');
  return (
    <div className="diff mono">
      {lines.map((ln, i) => {
        let cls = 'diff-line';
        if (ln.startsWith('@@')) cls += ' hunk';
        else if (ln.startsWith('+') && !ln.startsWith('+++')) cls += ' add';
        else if (ln.startsWith('-') && !ln.startsWith('---')) cls += ' del';
        return (
          <div className={cls} key={i}>
            <span className="ln">{i + 1}</span>
            <span>{ln || ' '}</span>
          </div>
        );
      })}
    </div>
  );
}
