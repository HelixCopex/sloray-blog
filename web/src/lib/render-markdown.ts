/** 公开文章、独立页面和写作预览使用同一服务端渲染与清洗流程。 */
import { createMarkdownProcessor } from '@astrojs/markdown-remark';
import rehypeSanitize from 'rehype-sanitize';
import { articleHeadings } from './markdown';

const processor = createMarkdownProcessor({ rehypePlugins: [rehypeSanitize, articleHeadings] });
export async function renderMarkdown(markdown: string): Promise<string> {
  return (await (await processor).render(markdown)).code;
}
