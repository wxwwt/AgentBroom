import {createContext, useContext, useEffect, useState, type ReactNode} from "react";
import {isTauri} from "@tauri-apps/api/core";
import {getCurrentWindow} from "@tauri-apps/api/window";
import {zhMessages} from "./locales/zh-CN";
export type Locale = "en" | "zh-CN";
export type MessageKey = keyof typeof zhMessages;
export type MessageValues = Record<string, string | number>;
export const LANGUAGE_KEY = "agentbroom.language";
export function chooseLocale(saved: string | null, languages: readonly string[]): Locale {
  if (saved === "en" || saved === "zh-CN") return saved;
  for (const tag of languages) {
    if (/^zh(?:-|$)/i.test(tag)) return "zh-CN";
    if (/^en(?:-|$)/i.test(tag)) return "en";
  }
  return "en";
}
function initialLocale(): Locale {
  let saved: string | null = null;
  try { saved = localStorage.getItem(LANGUAGE_KEY); } catch { /* Storage may be disabled; system preference still works. */ }
  return chooseLocale(saved, navigator.languages ?? [navigator.language]);
}
const singular: Partial<Record<MessageKey,string>> = {
  "{count} files": "{count} file", "{count} segments": "{count} segment",
  "{count} selected conversations": "{count} selected conversation",
};
export function translate(locale: Locale, key: MessageKey, values: MessageValues = {}): string {
  const pattern = locale === "zh-CN" ? zhMessages[key] : values.count === 1 ? singular[key] ?? key : key;
  return pattern.replace(/\{(\w+)\}/g, (token, name: string) => values[name] === undefined ? token : String(values[name]));
}
/** Display-only lookup for known system metadata. Never use on titles, paths, IDs or reports. */
export function translateSystem(locale: Locale, text: string): string {
  return Object.hasOwn(zhMessages,text) ? translate(locale,text as MessageKey) : text;
}
interface LocaleContextValue {
  locale: Locale;
  setLocale: (locale: Locale) => void;
  persistenceUnavailable: boolean;
  t: (key: MessageKey, values?: MessageValues) => string;
  systemText: (text: string) => string;
}
const LocaleContext = createContext<LocaleContextValue | null>(null);
export function LocaleProvider({children}: {children: ReactNode}) {
  const [locale, updateLocale] = useState<Locale>(initialLocale);
  const [persistenceUnavailable, setUnavailable] = useState(false);
  function setLocale(next: Locale) {
    updateLocale(next);
    try { localStorage.setItem(LANGUAGE_KEY,next);setUnavailable(false); }
    catch { setUnavailable(true); }
  }
  useEffect(() => {
    document.documentElement.lang = locale;
    const title = translate(locale,"AgentBroom · Read-only prototype");
    document.title = title;
    if (isTauri()) void getCurrentWindow().setTitle(title).catch(() => { /* The page remains localized if a platform cannot update its title. */ });
  },[locale]);
  return <LocaleContext.Provider value={{locale,setLocale,persistenceUnavailable,t:(key,values)=>translate(locale,key,values),systemText:text=>translateSystem(locale,text)}}>{children}</LocaleContext.Provider>;
}
export function useLocale(): LocaleContextValue {
  const value = useContext(LocaleContext);
  if (!value) throw new Error("LocaleProvider is required.");
  return value;
}

export function LanguageSelector() {
  const {locale,setLocale,t} = useLocale();
  return <label className="language-control">{t("Language")}<select aria-label={t("Language")} value={locale} onChange={e=>setLocale(e.target.value as Locale)}><option value="zh-CN" lang="zh-CN">中文（简体）</option><option value="en" lang="en">English</option></select></label>;
}
