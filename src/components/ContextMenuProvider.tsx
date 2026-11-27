import { Copy, Scissors, TextSelect } from "lucide-react";
import {
  createContext,
  type ReactNode,
  useCallback,
  useContext,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
} from "react";

export type ContextMenuEntry =
  | {
      type?: "item";
      id: string;
      label: string;
      icon?: ReactNode;
      shortcut?: string;
      danger?: boolean;
      disabled?: boolean;
      onSelect: () => void | Promise<void>;
    }
  | {
      type: "separator";
      id: string;
    }
  | {
      type: "heading";
      id: string;
      label: string;
    };

interface ContextMenuState {
  x: number;
  y: number;
  items: ContextMenuEntry[];
}

interface ContextMenuContextValue {
  openContextMenu: (
    coords: { clientX: number; clientY: number },
    items: ContextMenuEntry[],
  ) => void;
  closeContextMenu: () => void;
}

const ContextMenuContext = createContext<ContextMenuContextValue | null>(null);

export function useGlobalContextMenu() {
  const ctx = useContext(ContextMenuContext);
  if (!ctx) {
    throw new Error(
      "useGlobalContextMenu must be used within a ContextMenuProvider",
    );
  }
  return ctx;
}

function setNativeInputValue(
  element: HTMLInputElement | HTMLTextAreaElement,
  nextValue: string,
) {
  const proto =
    element instanceof HTMLTextAreaElement
      ? HTMLTextAreaElement.prototype
      : HTMLInputElement.prototype;
  const descriptor = Object.getOwnPropertyDescriptor(proto, "value");
  descriptor?.set?.call(element, nextValue);
  element.dispatchEvent(new Event("input", { bubbles: true }));
}

function isEditableTextElement(
  target: EventTarget | null,
): target is HTMLInputElement | HTMLTextAreaElement {
  if (target instanceof HTMLTextAreaElement) {
    return !target.disabled;
  }
  if (target instanceof HTMLInputElement) {
    if (target.disabled) return false;
    return [
      "text",
      "search",
      "url",
      "email",
      "password",
      "tel",
      "number",
    ].includes(target.type);
  }
  return false;
}

function buildEditableMenuItems(
  element: HTMLInputElement | HTMLTextAreaElement,
): ContextMenuEntry[] {
  const start = element.selectionStart ?? 0;
  const end = element.selectionEnd ?? 0;
  const hasSelection = end > start;
  const selectedText = hasSelection ? element.value.slice(start, end) : "";
  const canEdit = !element.readOnly && !element.disabled;

  return [
    {
      id: "text-cut",
      label: "Cut",
      icon: <Scissors size={14} />,
      shortcut: "⌘X",
      disabled: !canEdit || !hasSelection,
      onSelect: async () => {
        if (!hasSelection) return;
        await navigator.clipboard.writeText(selectedText);
        const nextValue =
          element.value.slice(0, start) + element.value.slice(end);
        element.focus();
        setNativeInputValue(element, nextValue);
        element.setSelectionRange(start, start);
      },
    },
    {
      id: "text-copy",
      label: "Copy",
      icon: <Copy size={14} />,
      shortcut: "⌘C",
      disabled: !hasSelection,
      onSelect: async () => {
        if (!hasSelection) return;
        await navigator.clipboard.writeText(selectedText);
      },
    },
    {
      id: "text-paste",
      label: "Paste",
      icon: <Copy size={14} />,
      shortcut: "⌘V",
      disabled: !canEdit,
      onSelect: async () => {
        const clip = await navigator.clipboard.readText();
        const nextValue =
          element.value.slice(0, start) + clip + element.value.slice(end);
        element.focus();
        setNativeInputValue(element, nextValue);
        const caret = start + clip.length;
        element.setSelectionRange(caret, caret);
      },
    },
    { type: "separator", id: "text-sep" },
    {
      id: "text-select-all",
      label: "Select all",
      icon: <TextSelect size={14} />,
      shortcut: "⌘A",
      disabled: element.value.length === 0,
      onSelect: () => {
        element.focus();
        element.select();
      },
    },
  ];
}

interface ContextMenuProviderProps {
  children: ReactNode;
  getDefaultItems?: (target: HTMLElement | null) => ContextMenuEntry[];
}

export function ContextMenuProvider({
  children,
  getDefaultItems,
}: ContextMenuProviderProps) {
  const [menu, setMenu] = useState<ContextMenuState | null>(null);
  const [position, setPosition] = useState<{ left: number; top: number }>({
    left: 0,
    top: 0,
  });
  const menuRef = useRef<HTMLDivElement>(null);

  const closeContextMenu = useCallback(() => {
    setMenu(null);
  }, []);

  const openContextMenu = useCallback(
    (
      coords: { clientX: number; clientY: number },
      items: ContextMenuEntry[],
    ) => {
      if (items.length === 0) {
        setMenu(null);
        return;
      }
      setMenu({ x: coords.clientX, y: coords.clientY, items });
      setPosition({ left: coords.clientX, top: coords.clientY });
    },
    [],
  );

  useLayoutEffect(() => {
    if (!menu || !menuRef.current) return;
    const rect = menuRef.current.getBoundingClientRect();
    const clampedLeft = Math.max(
      8,
      Math.min(menu.x, window.innerWidth - rect.width - 8),
    );
    const clampedTop = Math.max(
      8,
      Math.min(menu.y, window.innerHeight - rect.height - 8),
    );
    setPosition({ left: clampedLeft, top: clampedTop });
  }, [menu]);

  useEffect(() => {
    const onGlobalContextMenu = (event: globalThis.MouseEvent) => {
      if (event.defaultPrevented) return;
      event.preventDefault();

      const target = event.target as HTMLElement | null;
      if (isEditableTextElement(target)) {
        openContextMenu(event, buildEditableMenuItems(target));
        return;
      }

      const selectionText = window.getSelection()?.toString() ?? "";
      if (selectionText.length > 0) {
        openContextMenu(event, [
          {
            id: "selection-copy",
            label: "Copy",
            icon: <Copy size={14} />,
            shortcut: "⌘C",
            onSelect: async () => {
              await navigator.clipboard.writeText(selectionText);
            },
          },
        ]);
        return;
      }

      // If a modal is open and target is inside it without a custom handler, don't show background empty-space menu.
      if (target?.closest(".ph-dialog-scrim")) {
        closeContextMenu();
        return;
      }

      const defaults = getDefaultItems?.(target) ?? [];
      openContextMenu(event, defaults);
    };

    document.addEventListener("contextmenu", onGlobalContextMenu);
    return () => {
      document.removeEventListener("contextmenu", onGlobalContextMenu);
    };
  }, [closeContextMenu, getDefaultItems, openContextMenu]);

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
    const dismissOnBlur = () => setMenu(null);

    window.addEventListener("pointerdown", dismissOnOutsideClick);
    window.addEventListener("keydown", dismissOnEscape);
    window.addEventListener("blur", dismissOnBlur);
    return () => {
      window.removeEventListener("pointerdown", dismissOnOutsideClick);
      window.removeEventListener("keydown", dismissOnEscape);
      window.removeEventListener("blur", dismissOnBlur);
    };
  }, [menu]);

  return (
    <ContextMenuContext.Provider value={{ openContextMenu, closeContextMenu }}>
      {children}
      {menu && (
        <div
          ref={menuRef}
          className="context-menu"
          data-context-menu=""
          role="menu"
          style={{ left: position.left, top: position.top }}
        >
          {menu.items.map((entry) => {
            if (entry.type === "separator") {
              return <div key={entry.id} className="menu-rule" />;
            }
            if (entry.type === "heading") {
              return (
                <div key={entry.id} className="context-heading">
                  {entry.label}
                </div>
              );
            }
            return (
              <button
                key={entry.id}
                type="button"
                role="menuitem"
                disabled={entry.disabled}
                className={entry.danger ? "danger-item" : undefined}
                onClick={() => {
                  closeContextMenu();
                  void entry.onSelect();
                }}
              >
                {entry.icon}
                <span>{entry.label}</span>
                {entry.shortcut && (
                  <span className="menu-shortcut">{entry.shortcut}</span>
                )}
              </button>
            );
          })}
        </div>
      )}
    </ContextMenuContext.Provider>
  );
}
