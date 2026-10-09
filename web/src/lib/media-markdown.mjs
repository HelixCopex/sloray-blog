/** A clipboard link must survive Markdown punctuation and angle destinations. */
export function mediaMarkdown(alt, imageUrl, dimensions) {
  const description = (alt.trim() || '图片').replace(/[\r\n]/g, ' ').replace(/[\\\[\]]/g, '\\$&');
  const url = imageUrl.replace(/[<>\s\\]/g, char => encodeURIComponent(char));
  const width = dimensions?.width, height = dimensions?.height;
  const sized = Number.isInteger(width) && Number.isInteger(height) && width > 0 && height > 0 && width <= 65535 && height <= 65535 && width * height <= 268435456;
  return `![${description}](<${url}>)${sized ? `{width=${width} height=${height}}` : ''}`;
}
