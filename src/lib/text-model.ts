/** 搜索索引、正文定位、高亮共用的标准文本与位置映射。 */
export interface TextNodeLike {
  nodeType: number;
  nodeName?: string;
  data?: string;
  childNodes: ArrayLike<TextNodeLike>;
}
export interface TextPoint { node: Text; offset: number }
const BLOCKS = new Set(["P", "DIV", "SECTION", "ARTICLE", "LI", "UL", "OL", "H1", "H2", "H3", "H4", "H5", "H6", "BLOCKQUOTE", "PRE", "TR", "TD", "TH", "HR"]);
const SKIP = new Set(["SCRIPT", "STYLE", "NOSCRIPT"]);

export function textModel(root: TextNodeLike): { text: string; points: TextPoint[] } {
  let text = "";
  const points: TextPoint[] = [];
  let space: TextPoint | null = null;
  const separate = () => { if (text.length) space = points[points.length - 1]; };
  const visit = (node: TextNodeLike) => {
    const tag = (node.nodeName ?? "").toUpperCase();
    if (SKIP.has(tag)) return;
    if (tag === "BR" || BLOCKS.has(tag)) separate();
    if (node.nodeType === 3) {
      const data = node.data ?? "";
      for (let offset = 0; offset < data.length; offset += 1) {
        const point = { node: node as unknown as Text, offset };
        if (/\s/.test(data[offset])) {
          if (text.length) space = point;
        } else {
          if (space && text.length) { text += " "; points.push(space); }
          space = null;
          text += data[offset];
          points.push(point);
        }
      }
    } else {
      Array.from(node.childNodes).forEach(visit);
    }
    if (BLOCKS.has(tag)) separate();
  };
  visit(root);
  return { text, points };
}

export function plainTextFromHtml(html: string): string {
  if (typeof DOMParser !== "undefined") {
    return textModel(new DOMParser().parseFromString(html, "text/html").body).text;
  }
  // 无 DOM 的单元测试使用轻量树；仍复用相同的文本归一化规则。
  const root: TextNodeLike = { nodeType: 1, nodeName: "BODY", childNodes: [] };
  const stack = [root];
  for (const token of html.match(/<!--[\s\S]*?-->|<[^>]*>|[^<]+/g) ?? []) {
    if (token.startsWith("<!--")) continue;
    if (token.startsWith("</")) {
      if (stack.length > 1) stack.pop();
    } else if (token.startsWith("<")) {
      const name = token.match(/^<\s*([\w-]+)/)?.[1]?.toUpperCase();
      if (!name) continue;
      const node: TextNodeLike = { nodeType: 1, nodeName: name, childNodes: [] };
      (stack[stack.length - 1].childNodes as TextNodeLike[]).push(node);
      if (!["BR", "IMG", "HR", "META", "LINK", "INPUT", "WBR"].includes(name) && !token.endsWith("/>")) stack.push(node);
    } else {
      const data = token.replace(/&(#x[\da-f]+|#\d+|nbsp|amp|lt|gt|quot|apos);/gi, (match, name: string) => {
        const entities: Record<string, string> = { nbsp: " ", amp: "&", lt: "<", gt: ">", quot: '"', apos: "'" };
        if (name.startsWith("#")) {
          const code = name[1].toLowerCase() === "x" ? parseInt(name.slice(2), 16) : parseInt(name.slice(1), 10);
          return code > 0 && code <= 0x10ffff ? String.fromCodePoint(code) : "\uFFFD";
        }
        return entities[name.toLowerCase()] ?? match;
      });
      (stack[stack.length - 1].childNodes as TextNodeLike[]).push({ nodeType: 3, data, childNodes: [] });
    }
  }
  return textModel(root).text;
}
