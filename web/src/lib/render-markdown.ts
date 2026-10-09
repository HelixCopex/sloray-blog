/** 公开文章、独立页面和写作预览使用同一服务端渲染与清洗流程。 */
import { createMarkdownProcessor } from '@astrojs/markdown-remark';
import rehypeSanitize from 'rehype-sanitize';
import { articleHeadings } from './markdown';

const processor = createMarkdownProcessor({ rehypePlugins: [rehypeSanitize, articleHeadings] });
export async function renderMarkdown(markdown: string): Promise<string> {
  return (await renderArticle(markdown)).code;
}
/** SSR 目录使用同一次清洗后的渲染结果，标题与锚点始终一致。 */
export async function renderArticle(markdown: string) {
  const result = await (await processor).render(markdown);
  return { code: result.code, headings: result.metadata.headings };
}
