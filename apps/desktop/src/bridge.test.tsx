import {afterEach,describe,expect,it,vi} from 'vitest';
import {testInventory} from '../test-fixtures/inventory';
afterEach(()=>{vi.unstubAllGlobals();vi.resetModules();});
describe('real-only browser bridge',()=>{
 it('shares concurrent reads and never calls fixture endpoints',async()=>{
  vi.stubGlobal('location',{hostname:'127.0.0.1'});
  let complete!:(value:unknown)=>void;const fetcher=vi.fn(()=>new Promise(resolve=>{complete=resolve;}));vi.stubGlobal('fetch',fetcher);
  const {loadInventory}=await import('./bridge');const a=loadInventory(),b=loadInventory();expect(a).toBe(b);expect(fetcher).toHaveBeenCalledTimes(1);
  expect(fetcher).toHaveBeenCalledWith('/__agentbroom/inventory',expect.objectContaining({headers:{'X-AgentBroom-ReadOnly':'1'},cache:'no-store'}));
  complete({ok:true,json:async()=>testInventory()});expect((await a).sourceKind).toBe('codex');
 });
 it('refuses synthetic results rather than showing a fallback',async()=>{
  vi.stubGlobal('location',{hostname:'localhost'});vi.stubGlobal('fetch',vi.fn(async()=>({ok:true,json:async()=>({...testInventory(),sourceKind:'demo'})})));
  const {loadInventory}=await import('./bridge');await expect(loadInventory()).rejects.toThrow('Only a real read-only Codex inventory');
 });
 it('blocks non-loopback browser origins before requesting metadata',async()=>{
  vi.stubGlobal('location',{hostname:'example.com'});const fetcher=vi.fn();vi.stubGlobal('fetch',fetcher);
  const {loadInventory}=await import('./bridge');await expect(loadInventory()).rejects.toThrow('loopback');expect(fetcher).not.toHaveBeenCalled();
 });
 it('retries after read errors without returning stale data',async()=>{
  vi.stubGlobal('location',{hostname:'localhost'});const fetcher=vi.fn().mockResolvedValueOnce({ok:false,json:async()=>({message:'Local inspection failed.'})}).mockResolvedValueOnce({ok:true,json:async()=>testInventory()});vi.stubGlobal('fetch',fetcher);
  const {loadInventory}=await import('./bridge');await expect(loadInventory()).rejects.toThrow('Local inspection failed.');expect((await loadInventory()).sourceKind).toBe('codex');expect(fetcher).toHaveBeenCalledTimes(2);
 });
});
