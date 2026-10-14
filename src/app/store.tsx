import {createContext,useContext,useSyncExternalStore,type ReactNode} from 'react';
import {DesktopRepository,desktop} from '../infrastructure/desktop';
import {AutosaveQueue} from '../infrastructure/autosave';
import {defaultSettings,planSchema,type CostPlan,type Settings} from '../core/model';
import {errorText} from '../shared/utils';
type Snapshot={plans:CostPlan[];settings:Settings;dataDirectory:string;loading:boolean;error:string;saveState:'saved'|'saving'|'error';message:string};
export class AppStore {
  private state:Snapshot={plans:[],settings:defaultSettings,dataDirectory:'',loading:true,error:'',saveState:'saved',message:''};private listeners=new Set<()=>void>();
  readonly repo=new DesktopRepository(); readonly saves=new AutosaveQueue(this.repo,(saveState,error)=>this.patch({saveState,...(error?{message:errorText(error)}:{})}));
  subscribe=(fn:()=>void)=>{this.listeners.add(fn);return()=>{this.listeners.delete(fn);};};snapshot=()=>this.state;
  patch=(v:Partial<Snapshot>)=>{this.state={...this.state,...v};this.listeners.forEach(fn=>fn());};
  notify=(e:unknown)=>this.patch({message:errorText(e)});
  async load(){try{const data=await this.repo.load();this.patch({...data,loading:false,error:''});}catch(e){this.patch({loading:false,error:desktop?errorText(e):'浏览器仅用于界面预览。请运行 ChengSuan.exe 使用本地数据库和桌面功能。'});}}
  update(plan:CostPlan){const valid=planSchema.parse({...plan,updatedAt:new Date().toISOString()});this.patch({plans:[valid,...this.state.plans.filter(p=>p.id!==valid.id)]});this.saves.schedule(valid);}
  async remove(id:string){await this.saves.remove(id);this.patch({plans:this.state.plans.filter(p=>p.id!==id)});}
  async settings(v:Partial<Settings>){await this.saves.flush();const settings={...this.state.settings,...v};await this.repo.saveSettings(settings);this.patch({settings});}
  async imported(plans:CostPlan[]){await this.saves.flush();await this.repo.importPlans(plans);await this.load();}
  flush=()=>this.saves.flush();
}
export const store=new AppStore();const Context=createContext(store);
export function StoreProvider({children}:{children:ReactNode}){return <Context.Provider value={store}>{children}</Context.Provider>;}
export function useApp(){const app=useContext(Context);const state=useSyncExternalStore(app.subscribe,app.snapshot);return {...state,app};}
