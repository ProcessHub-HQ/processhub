import { type MouseEvent, useEffect, useState } from "react";

export interface ContextMenuPosition {
  serviceId: string;
  x: number;
  y: number;
}

export function useContextMenu() {
  const [menu, setMenu] = useState<ContextMenuPosition | null>(null);

  useEffect(() => {
    if (!menu) return;
    const dismissOnOutsideClick = (event: PointerEvent) => {
      if (
        !(event.target instanceof Element) ||
        !event.target.closest("[data-context-menu]")
      ) {
        setMenu(null);
      }
    };
    const dismissOnEscape = (event: KeyboardEvent) => {
      if (event.key === "Escape") setMenu(null);
    };
    window.addEventListener("pointerdown", dismissOnOutsideClick);
    window.addEventListener("keydown", dismissOnEscape);
    return () => {
      window.removeEventListener("pointerdown", dismissOnOutsideClick);
      window.removeEventListener("keydown", dismissOnEscape);
    };
  }, [menu]);

  function open(
    event: Pick<MouseEvent, "preventDefault" | "clientX" | "clientY">,
    serviceId: string,
  ) {
    event.preventDefault();
    setMenu({
      serviceId,
      x: Math.max(8, Math.min(event.clientX, window.innerWidth - 224)),
      y: Math.max(8, Math.min(event.clientY, window.innerHeight - 270)),
    });
  }

  return { menu, open, close: () => setMenu(null) };
}
