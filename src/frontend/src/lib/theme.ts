import type { Settings, ThemeColors, ThemeMode } from "./api";

export const themeDefaults: Record<ThemeMode, ThemeColors> = {
  dark: {
    textColor: "#d4d4d4",
    backgroundColor: "#1e1f22",
    panelColor: "#26282b",
    accentColor: "#5aa0f2",
    headingColor: "#d4d4d4",
  },
  light: {
    textColor: "#303640",
    backgroundColor: "#fafbfc",
    panelColor: "#eceef1",
    accentColor: "#245fa8",
    headingColor: "#202630",
  },
};

export function switchTheme(settings: Settings, mode: ThemeMode) {
  if (settings.themeMode === mode) return;
  const { textColor, backgroundColor, panelColor, accentColor, headingColor } = settings;
  settings.themeColors[settings.themeMode] = {
    textColor, backgroundColor, panelColor, accentColor, headingColor,
  };
  Object.assign(settings, settings.themeColors[mode] ?? themeDefaults[mode]);
  settings.themeMode = mode;
}
