import {
  createContext,
  useContext,
  useEffect,
  useState,
  type ReactNode,
} from "react";
import { Moon, Sun } from "lucide-react";
import { useLocale } from "./i18n";
export type Theme = "light" | "dark";
export const THEME_KEY = "agentbroom.theme";
function initial(): Theme {
  try {
    const saved = localStorage.getItem(THEME_KEY);
    if (saved === "light" || saved === "dark") return saved;
  } catch {}
  return window.matchMedia?.("(prefers-color-scheme: dark)").matches
    ? "dark"
    : "light";
}
const Context = createContext<{
  theme: Theme;
  toggle: () => void;
  unavailable: boolean;
} | null>(null);
export function ThemeProvider({ children }: { children: ReactNode }) {
  const [theme, setTheme] = useState<Theme>(initial),
    [unavailable, setUnavailable] = useState(false);
  useEffect(() => {
    document.documentElement.dataset.theme = theme;
    document.documentElement.style.colorScheme = theme;
  }, [theme]);
  const toggle = () =>
    setTheme((current) => {
      const next = current === "light" ? "dark" : "light";
      try {
        localStorage.setItem(THEME_KEY, next);
        setUnavailable(false);
      } catch {
        setUnavailable(true);
      }
      return next;
    });
  return (
    <Context.Provider value={{ theme, toggle, unavailable }}>
      {children}
    </Context.Provider>
  );
}
export function ThemeToggle() {
  const value = useContext(Context)!;
  const { t } = useLocale();
  const Icon = value.theme === "light" ? Moon : Sun;
  return (
    <>
      <button
        className="icon-button theme-toggle"
        aria-label={t(
          value.theme === "light"
            ? "Switch to dark mode"
            : "Switch to light mode",
        )}
        onClick={value.toggle}
      >
        <Icon size={18} />
      </button>
      {value.unavailable && (
        <span role="status" className="muted">
          {t("Theme preference could not be saved.")}
        </span>
      )}
    </>
  );
}
