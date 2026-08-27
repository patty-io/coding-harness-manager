import { useEffect } from "react";
import { useNavigate } from "react-router-dom";

/**
 * Browser-style history navigation inside the webview:
 * - mouse back/forward buttons (buttons 3 and 4)
 * - Cmd/Ctrl+Left, Cmd/Ctrl+Right
 * - Cmd/Ctrl+[ and Cmd/Ctrl+]
 *
 * A short lock prevents double-firing when the webview handles a mouse
 * button natively *and* our listener fires for the same press.
 */
export function useBackForwardNavigation() {
  const navigate = useNavigate();

  useEffect(() => {
    let lockUntil = 0;
    const locked = () => performance.now() < lockUntil;
    const go = (delta: number) => {
      if (locked()) return;
      lockUntil = performance.now() + 250;
      navigate(delta);
    };

    const onMouseUp = (e: MouseEvent) => {
      if (e.button === 3) {
        e.preventDefault();
        go(-1);
      } else if (e.button === 4) {
        e.preventDefault();
        go(1);
      }
    };

    const onKeyDown = (e: KeyboardEvent) => {
      const mod = e.metaKey || e.ctrlKey;
      if (!mod) return;
      if (e.key === "ArrowLeft" || e.key === "[") {
        e.preventDefault();
        go(-1);
      } else if (e.key === "ArrowRight" || e.key === "]") {
        e.preventDefault();
        go(1);
      }
    };

    window.addEventListener("mouseup", onMouseUp);
    window.addEventListener("keydown", onKeyDown);
    return () => {
      window.removeEventListener("mouseup", onMouseUp);
      window.removeEventListener("keydown", onKeyDown);
    };
  }, [navigate]);
}