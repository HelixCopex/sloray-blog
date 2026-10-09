import { serverGet, type PageList, type PageSummary } from './api';

export async function allPublicPages(): Promise<PageSummary[]> {
  const pages: PageSummary[] = [];
  const seen = new Set<string>();
  let cursor: string | null | undefined;
  do {
    const page = await serverGet<PageList>(`/pages?limit=50${cursor ? `&cursor=${encodeURIComponent(cursor)}` : ''}`);
    pages.push(...page.items);
    cursor = page.next_cursor;
    if (cursor) {
      if (seen.has(cursor)) throw new Error('重复的页面游标');
      seen.add(cursor);
    }
  } while (cursor);
  return pages;
}
