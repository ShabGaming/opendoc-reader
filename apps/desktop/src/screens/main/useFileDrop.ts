import { useEffect, useState } from "react";
import { useApp } from "../../state/AppContext";

/** True while files are dragged over the window. Dropped paths go to the queue. */
export function useFileDrop(active: boolean): boolean {
  const { platform, actions } = useApp();
  const [over, setOver] = useState(false);
  const { addPaths } = actions;

  useEffect(() => {
    if (!active) return;
    let off: (() => void) | null = null;
    let cancelled = false;
    platform
      .onFileDrop({
        onOver: setOver,
        onDrop: (paths) => void addPaths(paths),
      })
      .then((unsubscribe) => {
        if (cancelled) unsubscribe();
        else off = unsubscribe;
      })
      .catch(() => {});
    return () => {
      cancelled = true;
      off?.();
      setOver(false);
    };
  }, [active, platform, addPaths]);

  return over;
}
