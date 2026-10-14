import {type CostPlan,type PlanRepository} from '../core/model';
export class AutosaveQueue {
  private pending=new Map<string,CostPlan>(); private timer:ReturnType<typeof setTimeout>|undefined; private chain:Promise<void>=Promise.resolve(); private failure:unknown;
  constructor(private repository:PlanRepository,private status:(state:'saving'|'saved'|'error',error?:unknown)=>void,private delay=500){}
  schedule(plan:CostPlan) {this.pending.set(plan.id,structuredClone(plan));this.status('saving');clearTimeout(this.timer);this.timer=setTimeout(()=>{void this.flush().catch(()=>{});},this.delay);}
  async flush() { clearTimeout(this.timer); const batch=[...this.pending.values()];this.pending.clear();
    this.chain=this.chain.catch(()=>{}).then(async()=>{for(const plan of batch) {try {await this.repository.savePlan(plan);this.failure=undefined;}catch(error){if(!this.pending.has(plan.id))this.pending.set(plan.id,plan);this.failure=error;this.status('error',error);throw error;}} if(!this.pending.size)this.status('saved');});
    await this.chain; if(this.failure)throw this.failure;
  }
  async remove(id:string) {this.pending.delete(id);await this.flush();await this.repository.deletePlan(id);}
}
