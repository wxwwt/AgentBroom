import {afterEach,describe,expect,it,vi} from 'vitest';
import {cleanup,fireEvent,render,screen,within} from '@testing-library/react';
import {testInventory} from '../test-fixtures/inventory';
const bridge=vi.hoisted(()=>({load:vi.fn()}));
vi.mock('./bridge',()=>({sourceLabel:'Native Codex scanner',loadInventory:bridge.load,loadCleanupMode:async()=>"readOnly",errorText:(e:Error)=>e.message}));
import App from './App';
afterEach(()=>{cleanup();vi.resetAllMocks();});
describe('Codex relationships using synthetic metadata',()=>{
 it('bounds rows and navigates fork, parent and child links',async()=>{
  bridge.load.mockResolvedValue(testInventory(65));render(<App/>);await screen.findByText('Where the bytes live');
  expect(screen.getByText('63 confirmed sub Agents · 0 unresolved')).toBeVisible();fireEvent.click(screen.getByRole('button',{name:'Conversations 65'}));
  expect(screen.getAllByRole('button',{name:/Local synthetic/})).toHaveLength(50);fireEvent.click(screen.getByRole('button',{name:'Show 50 more'}));
  expect(screen.getAllByRole('button',{name:/Local synthetic/})).toHaveLength(65);
  fireEvent.change(screen.getByRole('combobox',{name:'Filter conversations'}),{target:{value:'fork'}});
  fireEvent.click(screen.getByRole('button',{name:/Local synthetic 1/}));expect(screen.getByText('Fork lineage')).toBeVisible();
  fireEvent.click(screen.getAllByRole('button',{name:'synthetic-0'})[0]);const aside=screen.getByText('Relationship evidence').closest('aside')!;
  expect(within(aside).getByRole('button',{name:'synthetic-2'})).toBeVisible();expect(screen.getAllByText('Read-only inspection: cleanup eligibility is not evaluated')).toHaveLength(1);
 });
 it('labels unmatched indexed bytes as unattributed rather than zero',async()=>{
  const inv=testInventory();inv.threads[0].files=[];inv.threads[0].relation='unknown';inv.threads[0].ownBytes=0;inv.threads[0].inclusiveBytes=0;bridge.load.mockResolvedValue(inv);
  render(<App/>);await screen.findByText('Where the bytes live');fireEvent.click(screen.getByRole('button',{name:/Conversations/}));
  fireEvent.change(screen.getByRole('textbox',{name:'Search conversations'}),{target:{value:'Local synthetic 0'}});expect(screen.getByText('Unattributed')).toBeVisible();
  fireEvent.click(screen.getByRole('button',{name:/Local synthetic 0/}));expect(screen.getAllByText('Unknown / unattributed')).toHaveLength(2);
 });
});
