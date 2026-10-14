/// <reference types="node" />
import {afterEach, describe, expect, it, vi} from "vitest";
import {cleanup, fireEvent, render, screen, within} from "@testing-library/react";
import {testInventory} from "../test-fixtures/inventory";
const bridge=vi.hoisted(()=>({load:vi.fn()}));
vi.mock("./bridge",()=>({sourceLabel:"Native Codex scanner",loadInventory:bridge.load,errorText:(e:Error)=>e.message}));
import {readFileSync} from "node:fs";
import App from "./App";
import {chooseLocale, LANGUAGE_KEY, translate, translateSystem} from "./i18n";
import {zhMessages} from "./locales/zh-CN";
afterEach(()=>{cleanup();vi.restoreAllMocks();});
async function start(){bridge.load.mockResolvedValue(testInventory());render(<App/>);await screen.findByText("Where the bytes live");}
function chinese(){fireEvent.change(screen.getByRole("combobox",{name:"Language"}),{target:{value:"zh-CN"}});}
describe("language selection",()=>{
 it("uses supported system languages and validates saved preferences",()=>{
  expect(chooseLocale(null,["zh-Hant-HK","en-US"])).toBe("zh-CN");
  expect(chooseLocale("en",["zh-CN"])).toBe("en");
  expect(chooseLocale("fr",["de-DE","en-GB"])).toBe("en");
  expect(chooseLocale("zh-CN",["en-US"])).toBe("zh-CN");
  expect(chooseLocale(null,["fr-FR"])).toBe("en");
 });
 it("preserves search and details without rescanning on a language switch",async()=>{
  await start();fireEvent.click(screen.getByRole("button",{name:/Conversations/}));fireEvent.change(screen.getByRole("textbox",{name:"Search conversations"}),{target:{value:"Local synthetic 0"}});
  fireEvent.click(screen.getByRole("button",{name:/Local synthetic 0/}));chinese();expect(screen.getByRole("textbox",{name:"搜索对话"})).toHaveValue("Local synthetic 0");
  expect(screen.getByText("关联依据")).toBeVisible();expect(document.title).toBe("AgentBroom · 只读原型");expect(bridge.load).toHaveBeenCalledTimes(1);
  fireEvent.change(screen.getByRole("combobox",{name:"语言"}),{target:{value:"en"}});expect(screen.getByText("Relationship evidence")).toBeVisible();
 });
 it("restores the language after remount without persisting any inventory",async()=>{
  await start();chinese();cleanup();render(<App/>);
  await screen.findByText("空间占用分布");expect(screen.getByRole("combobox",{name:"语言"})).toHaveValue("zh-CN");
  expect(Object.keys(localStorage)).toEqual([LANGUAGE_KEY]);
 });
 it("localizes existing events and protection reasons without translating user content",async()=>{
  await start();chinese();fireEvent.click(screen.getByRole("button",{name:/操作记录/}));
  expect(screen.getByText("已只读盘点本地 Codex")).toBeVisible();expect(screen.getByText(/已测量 13 个文件/)).toBeVisible();
  fireEvent.click(screen.getByRole("button",{name:/对话/}));
  fireEvent.click(screen.getByRole("button",{name:/Local synthetic 0/}));
  expect(screen.getByText("仅做只读盘点，不评估是否符合清理条件")).toBeVisible();
  expect(screen.getByText("atlas-web")).toBeVisible();
  expect(screen.queryByRole("checkbox")).not.toBeInTheDocument();
 });
 it("keeps switching usable when preference storage is blocked",async()=>{
  await start();vi.spyOn(Storage.prototype,"setItem").mockImplementation(()=>{throw new Error("Blocked");});chinese();
  expect(screen.getByText("空间占用分布")).toBeVisible();expect(screen.getByRole("status")).toHaveTextContent("未能保存语言偏好");
 });
 it("keeps user values untranslated and does not expose reports",async()=>{
  await start();chinese();expect(screen.queryByRole("button",{name:/报告/})).not.toBeInTheDocument();
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
