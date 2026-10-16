import { useCallback, useEffect, useRef, type RefObject } from "react";

const SUSPEND_MS = 300;

/**
 * Keeps two scroll panes at the same fraction of their scroll height. `suspend` pauses the
 * link for 300 ms, so a programmatic jump in one pane does not drag the other back.
 */
export function useSyncedScroll(
  a: RefObject<HTMLElement | null>,
  b: RefObject<HTMLElement | null>,
  enabled: boolean,
): { suspend: () => void } {
  const until = useRef(0);
  const busy = useRef(false);

  const suspend = useCallback(() => {
    until.current = performance.now() + SUSPEND_MS;
  }, []);

  useEffect(() => {
    const first = a.current;
    const second = b.current;
    if (!enabled || !first || !second) return;

    const link = (from: HTMLElement, to: HTMLElement) => () => {
      if (busy.current || performance.now() < until.current) return;
      const span = from.scrollHeight - from.clientHeight;
      const ratio = span > 0 ? from.scrollTop / span : 0;
      busy.current = true;
      to.scrollTop = ratio * Math.max(0, to.scrollHeight - to.clientHeight);
      requestAnimationFrame(() => {
        busy.current = false;
      });
    };
    const onFirst = link(first, second);
    const onSecond = link(second, first);
    first.addEventListener("scroll", onFirst, { passive: true });
    second.addEventListener("scroll", onSecond, { passive: true });
    return () => {
      first.removeEventListener("scroll", onFirst);
      second.removeEventListener("scroll", onSecond);
    };
  }, [a, b, enabled]);

  return { suspend };
}
