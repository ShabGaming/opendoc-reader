import { Fragment, type ReactNode } from "react";
import { useApp } from "../../state/AppContext";
import { MathText } from "./math";

// Block text is inline Markdown: emphasis, code, links and inline math as $...$.
const TOKEN =
  /(\$[^$\n]+\$)|(`[^`\n]+`)|(\*\*[^*\n]+\*\*)|(\*[^*\n]+\*)|(\[[^\]\n]+\]\([^)\s]+\))/g;

function isSafeUrl(url: string): boolean {
  return /^https?:\/\//i.test(url);
}

function Link({ href, children }: { href: string; children: ReactNode }) {
  const { platform } = useApp();
  return (
    <a
      href={href}
      onClick={(event) => {
        event.preventDefault();
        void platform.openUrl(href);
      }}
    >
      {children}
    </a>
  );
}

function render(text: string, keyPrefix: string): ReactNode[] {
  const out: ReactNode[] = [];
  let last = 0;
  let n = 0;
  for (const match of text.matchAll(TOKEN)) {
    const at = match.index;
    if (at > last) out.push(text.slice(last, at));
    const token = match[0];
    const key = `${keyPrefix}${n++}`;
    if (match[1]) out.push(<MathText key={key} latex={token.slice(1, -1)} />);
    else if (match[2]) out.push(<code key={key}>{token.slice(1, -1)}</code>);
    else if (match[3]) out.push(<strong key={key}>{render(token.slice(2, -2), `${key}.`)}</strong>);
    else if (match[4]) out.push(<em key={key}>{render(token.slice(1, -1), `${key}.`)}</em>);
    else {
      const close = token.indexOf("](");
      const label = token.slice(1, close);
      const href = token.slice(close + 2, -1);
      out.push(
        isSafeUrl(href) ? (
          <Link key={key} href={href}>
            {label}
          </Link>
        ) : (
          label
        ),
      );
    }
    last = at + token.length;
  }
  if (last < text.length) out.push(text.slice(last));
  return out;
}

/** Renders inline Markdown without ever injecting raw HTML from the text. */
export function InlineText({ text }: { text: string }) {
  return <Fragment>{render(text, "i")}</Fragment>;
}
