/** Dimensions belong to body_md, so articles, pages and revisions share the same source. */
export interface ImageDimensions { width: number; height: number }
export interface ContentImage { url: string; start: number; end: number; width?: number; height?: number }
interface Node {
  type: string; tagName?: string; value?: string; url?: string; identifier?: string;
  position?: { start: { offset?: number }; end: { offset?: number } };
  children?: Node[]; properties?: Record<string, unknown>; data?: { hProperties?: Record<string, unknown> };
}
interface File { data: { astro?: { frontmatter?: Record<string, unknown> } } }
export function validDimensions(width: unknown, height: unknown): width is number {
  return typeof width === 'number' && typeof height === 'number' && Number.isInteger(width) && Number.isInteger(height) && width > 0 && height > 0 && width <= 65535 && height <= 65535 && width * height <= 268435456;
}
export function imageDimensions() {
  return (tree: Node, file: File) => {
    const definitions = new Map<string, string>();
    const images: ContentImage[] = [];
    function collect(node: Node) {
      if (node.type === 'definition' && node.identifier && node.url) definitions.set(node.identifier, node.url);
      node.children?.forEach(collect);
    }
    collect(tree);
    function visit(parent: Node) {
      parent.children?.forEach((node, index) => {
        const url = node.type === 'image' ? node.url : node.type === 'imageReference' ? definitions.get(node.identifier ?? '') : undefined;
        const start = node.position?.start.offset, end = node.position?.end.offset;
        if (url && start !== undefined && end !== undefined) {
          const image: ContentImage = { url, start, end };
          const next = parent.children![index + 1];
          const suffix = next?.type === 'text' && /^\{width=(\d+) height=(\d+)\}/.exec(next.value ?? '');
          if (suffix && validDimensions(Number(suffix[1]), Number(suffix[2]))) {
            image.width = Number(suffix[1]); image.height = Number(suffix[2]); image.end += suffix[0].length;
            node.data = { ...node.data, hProperties: { ...node.data?.hProperties, width: image.width, height: image.height } };
            next.value = next.value!.slice(suffix[0].length);
          }
          images.push(image);
        }
        visit(node);
      });
    }
    visit(tree);
    file.data.astro ??= { frontmatter: {} };
    file.data.astro.frontmatter ??= {};
    file.data.astro.frontmatter.contentImages = images;
  };
}
/** Runs after sanitization: only our own validated dimensions can enter a style attribute. */
export function imageFrames() {
  return (tree: Node, file: File) => {
    const allowed = new Map<number, string>();
    function visit(parent: Node) {
      parent.children?.forEach((node, index) => {
        if (node.type === 'element' && node.tagName === 'img' && typeof node.properties?.src === 'string') {
          const offset = node.position?.start.offset;
          if (offset !== undefined) allowed.set(offset, node.properties.src);
          const width = Number(node.properties.width), height = Number(node.properties.height);
          const sized = validDimensions(width, height);
          node.properties = { ...node.properties, decoding: 'async', loading: 'lazy', ...(sized ? { width, height } : {}) };
          parent.children![index] = { type: 'element', tagName: 'focus-image', properties: { className: ['image-frame'], ...(sized ? { style: `--image-aspect:${width}/${height};--image-width:${width}px` } : { 'data-unsized': '' }) }, children: [node] };
        } else visit(node);
      });
    }
    visit(tree);
    const images = file.data.astro?.frontmatter?.contentImages as ContentImage[] | undefined;
    if (images) file.data.astro!.frontmatter!.contentImages = images.filter(image => allowed.has(image.start)).map(image => ({ ...image, url: allowed.get(image.start)! }));
  };
}
