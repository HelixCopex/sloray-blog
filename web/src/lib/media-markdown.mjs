/** A clipboard link must survive Markdown punctuation and angle destinations. */
export function mediaMarkdown(alt, imageUrl) {
  const description = (alt.trim() || '图片').replace(/[\r\n]/g, ' ').replace(/[\\\[\]]/g, '\\$&');
  const url = imageUrl.replace(/[<>\s\\]/g, char => encodeURIComponent(char));
  return `![${description}](<${url}>)`;
}
