import { serverGet, type PostPage, type PostSummary } from './api';

export async function allPublicPosts(): Promise<PostSummary[]> {
  const posts: PostSummary[] = [];
  const seen = new Set<string>();
  let cursor: string | null | undefined;
  do {
    const page = await serverGet<PostPage>(`/posts?limit=50${cursor ? `&cursor=${encodeURIComponent(cursor)}` : ''}`);
    posts.push(...page.items);
    cursor = page.next_cursor;
    if (cursor) {
      if (seen.has(cursor)) throw new Error('重复的文章游标');
      seen.add(cursor);
    }
  } while (cursor);
  return posts;
}

export function xml(text: string): string {
  return text.replace(/[<>&"']/g, c => ({ '<': '&lt;', '>': '&gt;', '&': '&amp;', '"': '&quot;', "'": '&apos;' }[c]!));
}
