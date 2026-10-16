import katex from "katex";
import "katex/dist/katex.min.css";

const cache = new Map<string, string>();

/** KaTeX output as HTML. A formula it cannot parse is drawn as its source, not thrown. */
export function renderMath(latex: string, display: boolean): string {
  const key = `${display ? "d" : "i"}:${latex}`;
  let html = cache.get(key);
  if (html === undefined) {
    html = katex.renderToString(latex, {
      displayMode: display,
      throwOnError: false,
      strict: "ignore",
      trust: false,
      output: "htmlAndMathml",
    });
    cache.set(key, html);
  }
  return html;
}

interface MathTextProps {
  latex: string;
  display?: boolean;
}

export function MathText({ latex, display = false }: MathTextProps) {
  // renderToString escapes its input and the output is KaTeX's own markup.
  return (
    <span
      className={display ? "katex-display-host" : undefined}
      dangerouslySetInnerHTML={{ __html: renderMath(latex, display) }}
    />
  );
}
