import { invoke, isTauri } from '@tauri-apps/api/core';
import { planSchema,settingsSchema, type PlanRepository,type CostPlan,type Settings } from '../core/model';
import { parseRateResponse } from './exchange-rates';
export const desktop = isTauri();
export class DesktopRepository implements PlanRepository {
  async load() { const v=await invoke<{plans:unknown[];settings:unknown;dataDirectory:string}>('load_state');return {...v,plans:v.plans.map(p=>planSchema.parse(p)),settings:settingsSchema.parse(v.settings)}; }
  savePlan(plan:CostPlan) {return invoke<void>('save_plan',{plan:planSchema.parse(plan)});}
  deletePlan(id:string) {return invoke<void>('delete_plan',{id});}
  saveSettings(settings:Settings) {return invoke<void>('save_settings',{settings:settingsSchema.parse(settings)});}
  importPlans(plans:CostPlan[]) {return invoke<void>('import_plans',{plans:plans.map(p=>planSchema.parse(p))});}
}
export const native = {
  copy:(text:string)=>invoke<void>('copy_text',{text}),
  export:(extension:string,name:string,bytes:Uint8Array)=>invoke<string|null>('write_export',{extension,name,bytes:Array.from(bytes)}),
  readProject:()=>invoke<string|null>('read_project'),
  backup:()=>invoke<string>('create_backup'),
  restore:()=>invoke<boolean>('restore_backup'),
  rates:async(sources:string[],target:string)=>parseRateResponse(await invoke<unknown>('fetch_rates',{sources,target}))
};
