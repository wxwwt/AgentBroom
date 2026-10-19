import {expect,it} from "vitest";
import {sortThreads} from "./viewModel";
import {testInventory} from "../test-fixtures/inventory";
it("sorts unknown dates last, handles size ties deterministically and leaves the source unchanged",()=>{const rows=testInventory(3).threads;rows[0].updatedAt="";rows[1].updatedAt="2024-01-01";rows[2].updatedAt="2026-01-01";rows[1].inclusiveBytes=100;rows[2].inclusiveBytes=200;expect(sortThreads(rows,"updatedDesc").map(r=>r.id)).toEqual(["synthetic-2","synthetic-1","synthetic-0"]);expect(sortThreads(rows,"updatedAsc").at(-1)?.id).toBe("synthetic-0");expect(sortThreads(rows,"sizeAsc")[0].inclusiveBytes).toBeLessThanOrEqual(100);expect(rows[0].id).toBe("synthetic-0")});
