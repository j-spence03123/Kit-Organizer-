// Subscribes to native drag-and-drop on the Tauri webview (gives real
// filesystem paths, unlike the HTML5 drop event).
import { useEffect, useState } from "react";

export function useNativeDrop(onPaths: (paths: string[]) => void): boolean {
  const [over, setOver] = useState(false);
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    import("@tauri-apps/api/webview")
      .then(({ getCurrentWebview }) =>
        getCurrentWebview().onDragDropEvent((e) => {
          const p = e.payload;
          if (p.type === "enter" || p.type === "over") setOver(true);
          else if (p.type === "leave") setOver(false);
          else if (p.type === "drop") {
            setOver(false);
            if (p.paths.length) onPaths(p.paths);
          }
        }),
      )
      .then((u) => {
        if (cancelled) u();
        else unlisten = u;
      })
      .catch(() => undefined); // not running inside Tauri (tests)
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [onPaths]);
  return over;
}
