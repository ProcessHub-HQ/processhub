import { type KeyboardEvent, useRef } from "react";
import { type Appearance, useAppearanceStore } from "../stores/appearanceStore";

const OPTIONS: { value: Appearance; label: string }[] = [
  { value: "system", label: "System" },
  { value: "light", label: "Light" },
  { value: "dark", label: "Dark" },
];

export function AppearanceSwitch() {
  const appearance = useAppearanceStore((state) => state.appearance);
  const setAppearance = useAppearanceStore((state) => state.setAppearance);
  const buttons = useRef<Array<HTMLButtonElement | null>>([]);

  const select = (index: number) => {
    setAppearance(OPTIONS[index].value);
    buttons.current[index]?.focus();
  };

  const onKeyDown = (event: KeyboardEvent, index: number) => {
    let next: number;
    switch (event.key) {
      case "ArrowRight":
      case "ArrowDown":
        next = (index + 1) % OPTIONS.length;
        break;
      case "ArrowLeft":
      case "ArrowUp":
        next = (index - 1 + OPTIONS.length) % OPTIONS.length;
        break;
      case "Home":
        next = 0;
        break;
      case "End":
        next = OPTIONS.length - 1;
        break;
      default:
        return;
    }
    event.preventDefault();
    select(next);
  };

  return (
    <div
      className="appearance-switch"
      role="radiogroup"
      aria-label="Appearance"
    >
      {OPTIONS.map((option, index) => (
        <button
          key={option.value}
          ref={(node) => {
            buttons.current[index] = node;
          }}
          type="button"
          role="radio"
          aria-checked={appearance === option.value}
          tabIndex={appearance === option.value ? 0 : -1}
          className="appearance-switch__option"
          onClick={() => setAppearance(option.value)}
          onKeyDown={(event) => onKeyDown(event, index)}
        >
          {option.label}
        </button>
      ))}
    </div>
  );
}
