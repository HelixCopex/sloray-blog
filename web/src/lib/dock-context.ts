/** 页面给出资源身份；操作和鉴权在独立的 Dock / Rust 业务层解释。 */
export interface DockContext {
  kind: 'home' | 'post' | 'page' | 'search' | 'admin' | 'other';
  id?: string;
  slug?: string;
  author_id?: string;
}
export const pageIcons = [
  ['article','文章'], ['home','主页'], ['person','人物'], ['image','图片'], ['reading','阅读'],
  ['comment','讨论'], ['tag','标签'], ['rss','订阅'], ['preview','探索'], ['palette','创作'],
  ['settings','设置'], ['edit','书写'],
] as const;
