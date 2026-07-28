/**
 * @typedef {{ id: string, text: string, level: number }} TocItem
 * @typedef {{
 *   tagName: string,
 *   textContent?: string | null,
 *   id: string,
 *   classList: { add: (name: string) => void }
 * }} ArticleHeading
 */

/**
 * Assign stable, unique anchors to rendered article headings and return their
 * table-of-contents representation in document order.
 *
 * @param {Iterable<ArticleHeading>} headings
 * @returns {TocItem[]}
 */
export function buildArticleToc(headings) {
  const slugCounts = new Map();
  return Array.from(headings, (heading, index) => {
    const text = heading.textContent?.trim() || `小节 ${index + 1}`;
    const base = text.normalize("NFKC").toLocaleLowerCase()
      .replace(/[^\p{Letter}\p{Number}]+/gu, "-")
      .replace(/^-+|-+$/g, "") || `section-${index + 1}`;
    const count = (slugCounts.get(base) || 0) + 1;
    slugCounts.set(base, count);
    const id = `article-${base}${count > 1 ? `-${count}` : ""}`;
    heading.id = id;
    heading.classList.add("article-heading");
    return { id, text, level: Number(heading.tagName.slice(1)) };
  });
}

/**
 * Resolve the last heading that has crossed the reading guide line. Before the
 * first heading reaches that line, the first item remains selected.
 *
 * @param {TocItem[]} items
 * @param {(id: string) => number | null | undefined} getHeadingTop
 * @param {number} [guideLine]
 * @returns {string}
 */
export function getActiveTocId(items, getHeadingTop, guideLine = 150) {
  let currentId = items[0]?.id || "article-content";
  for (const item of items) {
    const top = getHeadingTop(item.id);
    if (typeof top !== "number" || Number.isNaN(top)) continue;
    if (top <= guideLine) currentId = item.id;
    else break;
  }
  return currentId;
}
