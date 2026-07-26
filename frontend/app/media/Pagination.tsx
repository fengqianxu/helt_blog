type PaginationProps = {
  current: number;
  count: number;
  total: number;
  unit: string;
  label: string;
  onChange: (page: number) => void;
};

function pageOptions(current: number, count: number) {
  const visible = Array.from(new Set([1, current - 1, current, current + 1, count]))
    .filter((item) => item >= 1 && item <= count)
    .sort((left, right) => left - right);
  const options: Array<number | string> = [];
  visible.forEach((item, index) => {
    if (index > 0 && item - visible[index - 1] > 1) options.push(`ellipsis-${item}`);
    options.push(item);
  });
  return options;
}

export function Pagination({ current, count, total, unit, label, onChange }: PaginationProps) {
  if (count <= 1) return null;
  return (
    <>
      <nav className="pagination bangumi-pagination" aria-label={label}>
        <button onClick={() => onChange(current - 1)} disabled={current === 1} aria-label="上一页">◀</button>
        {pageOptions(current, count).map((item) => typeof item === "number"
          ? <button key={item} className={current === item ? "current" : ""} onClick={() => onChange(item)} aria-current={current === item ? "page" : undefined} aria-label={`第 ${item} 页`}>{item}</button>
          : <span className="pagination-ellipsis" key={item} aria-hidden="true">…</span>)}
        <button onClick={() => onChange(current + 1)} disabled={current === count} aria-label="下一页">▶</button>
      </nav>
      <p className="bangumi-page-summary" aria-live="polite">第 {current} / {count} 页 · 共 {total} {unit}</p>
    </>
  );
}
