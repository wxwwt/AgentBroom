/// <reference types="node" />
import {afterEach, describe, expect, it, vi} from "vitest";
import {cleanup, fireEvent, render, screen, within} from "@testing-library/react";
import {readFileSync} from "node:fs";
import App from "./App";
import {chooseLocale, LANGUAGE_KEY, translate, translateSystem} from "./i18n";
import {zhMessages} from "./locales/zh-CN";
afterEach(()=>{cleanup();vi.restoreAllMocks();});
async function start(){render(<App/>);await screen.findByText("Where the bytes live");}
function chinese(){fireEvent.change(screen.getByRole("combobox",{name:"Language"}),{target:{value:"zh-CN"}});}
describe("language selection",()=>{
 it("uses supported system languages and validates saved preferences",()=>{
  expect(chooseLocale(null,["zh-Hant-HK","en-US"])).toBe("zh-CN");
  expect(chooseLocale("en",["zh-CN"])).toBe("en");
  expect(chooseLocale("fr",["de-DE","en-GB"])).toBe("en");
  expect(chooseLocale("zh-CN",["en-US"])).toBe("zh-CN");
  expect(chooseLocale(null,["fr-FR"])).toBe("en");
 });
 it("switches all visible UI and preserves selection, search and protected state",async()=>{
  await start();fireEvent.click(screen.getByRole("button",{name:/Conversations/}));
  fireEvent.change(screen.getByRole("textbox",{name:"Search conversations"}),{target:{value:"Orbit"}});
  fireEvent.click(screen.getByRole("checkbox",{name:"Select Refactor the Orbit API"}));
  chinese();expect(screen.getByRole("textbox",{name:"搜索对话"})).toHaveValue("Orbit");
  expect(screen.getByRole("checkbox",{name:"选择 Refactor the Orbit API"})).toBeChecked();
  expect(screen.getByRole("button",{name:"查看选中记录"})).toBeEnabled();
  expect(document.documentElement.lang).toBe("zh-CN");expect(document.title).toBe("AgentBroom · 只读原型");
  expect(localStorage.getItem(LANGUAGE_KEY)).toBe("zh-CN");
  fireEvent.click(screen.getByRole("button",{name:"查看选中记录"}));
  const modal=await screen.findByRole("dialog");expect(within(modal).getByText("执行功能已禁用。")).toBeVisible();
  fireEvent.change(screen.getByRole("combobox",{name:"语言"}),{target:{value:"en"}});
  expect(within(modal).getByText("Execution is disabled.")).toBeVisible();
 });
 it("restores the language after remount without persisting any inventory",async()=>{
  await start();chinese();cleanup();render(<App/>);
  await screen.findByText("空间占用分布");expect(screen.getByRole("combobox",{name:"语言"})).toHaveValue("zh-CN");
  expect(Object.keys(localStorage)).toEqual([LANGUAGE_KEY]);
 });
 it("localizes existing events and protection reasons without translating user content",async()=>{
  await start();chinese();fireEvent.click(screen.getByRole("button",{name:/操作记录/}));
  expect(screen.getByText("已盘点演示数据")).toBeVisible();expect(screen.getByText(/已查看 13 个合成样本文件/)).toBeVisible();
  fireEvent.click(screen.getByRole("button",{name:/对话/}));
  fireEvent.click(screen.getByRole("button",{name:/Build the Atlas dashboard/}));
  expect(screen.getByText("被独立 fork 引用")).toBeVisible();
  expect(screen.getByText("atlas-web")).toBeVisible();
  expect(screen.getByRole("checkbox",{name:"选择 Build the Atlas dashboard"})).toBeDisabled();
 });
 it("keeps switching usable when preference storage is blocked",async()=>{
  await start();vi.spyOn(Storage.prototype,"setItem").mockImplementation(()=>{throw new Error("Blocked");});chinese();
  expect(screen.getByText("空间占用分布")).toBeVisible();expect(screen.getByRole("status")).toHaveTextContent("未能保存语言偏好");
 });
 it("keeps JSON reports and identity values unchanged across languages",async()=>{
  await start();fireEvent.click(screen.getByRole("button",{name:"View sample report"}));
  const area=await screen.findByRole("textbox",{name:"Sample JSON report"});const before=(area as HTMLTextAreaElement).value;
  chinese();expect(screen.getByRole("textbox",{name:"样本 JSON 报告"})).toHaveValue(before);
  expect(translate("zh-CN","Select {title}",{title:"Overview /private/project"})).toBe("选择 Overview /private/project");
  expect(translateSystem("zh-CN","New unknown vendor status")).toBe("New unknown vendor status");
 });
 it("covers current scanner system messages and preserves interpolation tokens",()=>{
  for (const [en,zh] of Object.entries(zhMessages)) {
   expect([...en.matchAll(/\{\w+\}/g)].map(m=>m[0]).sort()).toEqual([...zh.matchAll(/\{\w+\}/g)].map(m=>m[0]).sort());
   expect(zh.trim()).not.toBe("");
  }
  for(const file of ["crates/core/src/lib.rs","crates/core/src/codex.rs","apps/desktop/src-tauri/src/lib.rs","apps/desktop/src/bridge.ts"]) {
   const testUrl=import.meta.url;const source=readFileSync(new URL(`../../../${file}`,testUrl),"utf8");
   for(const match of source.matchAll(/"((?:[^"\\]|\\.)*)"/g)) {
    const text=match[1];if(text.length < 13 || !text.includes(" ")||text.includes("{")||!/[A-Za-z]/.test(text))continue;
    expect(Object.hasOwn(zhMessages,text),`${file}: ${text}`).toBe(true);
   }
  }
 });
});
