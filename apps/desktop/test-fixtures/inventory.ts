import catalog from './catalog.json';
import type {Inventory,Thread} from '../src/types';
/** Synthetic testing only. Production code must never import this module. */
export function testInventory(count=7): Inventory {
 const base=catalog.inventory.threads[0] as Thread;
 return {...structuredClone(catalog.inventory) as Inventory, sourceKind:'codex', partialCoverage:true,
   datasetLabel:'Synthetic test metadata',sourceFingerprint:'synthetic-live',
   threads:Array.from({length:count},(_,i)=>({...base,id:`synthetic-${i}`,title:`Local synthetic ${i}`,relation:i===0?'root':i===1?'fork':'spawned',parentId:i===0?null:'synthetic-0',sourceType:'user',ownedDescendantIds:i===0?['synthetic-2']:[],forkSources:i===1?['synthetic-0']:[],protectionReasons:['Read-only inspection: cleanup eligibility is not evaluated','Live status unknown: no runtime or database activity queried']})),
 };
}
