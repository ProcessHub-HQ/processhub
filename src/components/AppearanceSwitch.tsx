import { type Appearance, useAppearanceStore } from "../stores/appearanceStore";
import { SegmentedControl } from "./SegmentedControl";

const OPTIONS: { value: Appearance; label: string }[] = [
  { value: "system", label: "System" },
  { value: "light", label: "Light" },
  { value: "dark", label: "Dark" },
];

export function AppearanceSwitch() {
  const appearance = useAppearanceStore((state) => state.appearance);
  const setAppearance = useAppearanceStore((state) => state.setAppearance);

  return (
    <SegmentedControl
      label="Appearance"
      value={appearance}
      options={OPTIONS}
      onChange={setAppearance}
    />
  );
}
