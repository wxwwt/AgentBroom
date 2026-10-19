import {execFile} from "node:child_process";
import {promisify} from "node:util";
import {resolve} from "node:path";
import type {IncomingMessage, ServerResponse} from "node:http";
import type {Plugin} from "vite";
const execute = promisify(execFile);
export const INVENTORY_PATH = '/__agentbroom/inventory';
const allowedHosts = new Set(['127.0.0.1:1420','localhost:1420']);
export function allowedRead(req: IncomingMessage,hosts=allowedHosts): boolean {
  const host=req.headers.host ?? '';
  const origin=req.headers.origin;
  return req.method === 'GET' && hosts.has(host)
    && ['127.0.0.1','::1','::ffff:127.0.0.1'].includes(req.socket.remoteAddress ?? '')
    && req.headers['x-agentbroom-readonly'] === '1'
    && (!origin || origin === `http://${host}`)
    && (!req.headers['sec-fetch-site'] || req.headers['sec-fetch-site'] === 'same-origin');
}
export function createInventoryHandler(scan: ()=>Promise<string>,hosts=allowedHosts) {
  let pending: Promise<string> | null = null;
  return async (req: IncomingMessage, res: ServerResponse, next: ()=>void) => {
    if (req.url?.split('?')[0] !== INVENTORY_PATH) return next();
    res.setHeader('Cache-Control','no-store');
    res.setHeader('Content-Type','application/json');
    res.setHeader('X-Content-Type-Options','nosniff');
    if (!allowedRead(req,hosts)) {
      res.statusCode=403;res.end(JSON.stringify({message:'Local read-only inventory requests must originate from this application.'}));return;
    }
    try {
      pending ??= scan().finally(()=>{pending=null;});
      const body=await pending;
      const value=JSON.parse(body);
      if (value.sourceKind !== 'codex' || value.executionEnabled !== false) throw new Error('Invalid inventory');
      res.end(body);
    } catch {
      res.statusCode=503;res.end(JSON.stringify({message:'Local inspection failed.'}));
    }
  };
}
/** Development only. The native app uses scan_codex IPC and does not host HTTP. */
export function localInventoryPlugin(repoRoot: string): Plugin {
  return {
    name:'agentbroom-local-readonly',
    async configureServer(server) {
      await execute(process.env.CARGO ?? 'cargo',['build','--locked','-p','agentbroom-cli'],{cwd:repoRoot,maxBuffer:8*1024*1024});
      const binary=resolve(process.env.CARGO_TARGET_DIR ?? resolve(repoRoot,'target'),'debug',process.platform==='win32'?'agentbroom.exe':'agentbroom');
      const port=server.config.server.port??1420;
      const hosts=new Set([`127.0.0.1:${port}`,`localhost:${port}`]);
      server.middlewares.use(createInventoryHandler(async()=>{
        const result=await execute(binary,['scan'],{cwd:repoRoot,timeout:60_000,maxBuffer:64*1024*1024});
        return result.stdout;
      },hosts));
    },
  };
}
