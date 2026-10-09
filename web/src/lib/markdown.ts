/** 文章标题是页面唯一 h1；Markdown 标题在文章内部从 h2 起。 */
interface Element { type: string; tagName?: string; children?: Element[]; properties?: Record<string, unknown> }
export function articleHeadings() {
  return (tree: Element) => {
    function visit(node: Element) {
      if (node.type === 'element' && /^h[1-6]$/.test(node.tagName ?? '')) {
        node.tagName = `h${Math.min(Number(node.tagName!.slice(1)) + 1, 6)}`;
      }
      if (node.type === 'element' && node.tagName === 'pre') {
        node.properties = { ...node.properties, tabIndex: 0, ariaLabel: '代码块，可水平滚动' };
      }
      node.children?.forEach(visit);
    }
    visit(tree);
  };
}
