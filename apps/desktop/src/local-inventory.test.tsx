/// <reference types="node" />
import {describe,expect,it,vi} from 'vitest';
import type {IncomingMessage,ServerResponse} from 'node:http';
import {allowedRead,createInventoryHandler,INVENTORY_PATH} from '../local-inventory';
function request(extra:Record<string,unknown>={}): IncomingMessage {
 return {method:'GET',url:INVENTORY_PATH,headers:{host:'127.0.0.1:1420','x-agentbroom-readonly':'1','sec-fetch-site':'same-origin'},socket:{remoteAddress:'127.0.0.1'},...extra} as unknown as IncomingMessage;
}
function response(){const res={setHeader:vi.fn(),end:vi.fn(),statusCode:200};return res;}
describe('loopback metadata boundary',()=>{
 it('accepts only local, same-origin GET reads with the application header',()=>{
  expect(allowedRead(request())).toBe(true);
  for(const method of ['POST','DELETE','PUT','OPTIONS'])expect(allowedRead(request({method}))).toBe(false);
  for(const host of ['attacker.example','192.168.1.2:1420'])expect(allowedRead(request({headers:{host,'x-agentbroom-readonly':'1'}}))).toBe(false);
  expect(allowedRead(request({headers:{host:'127.0.0.1:1420'}}))).toBe(false);
  expect(allowedRead(request({headers:{host:'127.0.0.1:1420','x-agentbroom-readonly':'1',origin:'https://evil.example'}}))).toBe(false);
  expect(allowedRead(request({headers:{host:'127.0.0.1:1420','x-agentbroom-readonly':'1','sec-fetch-site':'cross-site'}}))).toBe(false);
  expect(allowedRead(request({socket:{remoteAddress:'192.168.1.2'}}))).toBe(false);
 });
 it('never starts the scanner for rejected requests and does not expose CORS',async()=>{
  const scan=vi.fn();const handler=createInventoryHandler(scan),res=response();await handler(request({method:'DELETE'}),res as unknown as ServerResponse,vi.fn());
  expect(res.statusCode).toBe(403);expect(scan).not.toHaveBeenCalled();expect(res.setHeader).not.toHaveBeenCalledWith('Access-Control-Allow-Origin',expect.anything());
 });
 it('shares one scan, ignores root query parameters and does not cache responses',async()=>{
  let finish!:(v:string)=>void;const scan=vi.fn(()=>new Promise<string>(resolve=>{finish=resolve;}));const handler=createInventoryHandler(scan),a=response(),b=response();
  const x=handler(request({url:INVENTORY_PATH+'?root=/private/ignored'}),a as unknown as ServerResponse,vi.fn()),y=handler(request(),b as unknown as ServerResponse,vi.fn());
  expect(scan).toHaveBeenCalledTimes(1);expect(scan).toHaveBeenCalledWith();finish(JSON.stringify({sourceKind:'codex',executionEnabled:false}));await Promise.all([x,y]);
  expect(a.end).toHaveBeenCalledWith(b.end.mock.calls[0][0]);expect(a.setHeader).toHaveBeenCalledWith('Cache-Control','no-store');
 });
 it('rejects fixtures and scan failures without leaking process error output',async()=>{
  for(const scan of [async()=>'{"sourceKind":"demo","executionEnabled":false}',async()=>{throw new Error('Private stderr');}]){
   const res=response();await createInventoryHandler(scan)(request(),res as unknown as ServerResponse,vi.fn());expect(res.statusCode).toBe(503);expect(res.end.mock.calls[0][0]).not.toContain('Private stderr');
  }
 });
});
