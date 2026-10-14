import ExcelJS from 'exceljs';
import { z } from 'zod';
import { planSchema, type CalculationResult, type CostPlan, PERIODS, periodLabels, type Settings } from '../../core/model';
import { money } from '../../core/calculator';

export type ExportFormat = Settings['lastFormat'];
export interface Exporter { format: ExportFormat; render(results: readonly CalculationResult[], precision: number | null): Promise<Uint8Array> }
export const exportScope = (results: readonly CalculationResult[]) => { if (!results.length) throw Error('请选择至少一个方案'); if (results.some(r => !r.complete)) throw Error('汇率缺失，无法导出完整金额报告。请获取或手动补齐汇率。'); };
export function summary(results: readonly CalculationResult[], full = false, precision: number | null = null): string {
  exportScope(results);
  return results.map(r => [
    `澄算 ChengSuan · ${r.plan.name}`, `汇总币种：${r.currency}`, r.plan.description,
    ...PERIODS.map(p => `${periodLabels[p]}等效平均成本：${money(r.total[p], r.currency, precision)}`),
    `一次性费用原额合计（另列，不重复计入）：${money(r.upfront,r.currency,precision)}`,
    ...(full ? r.items.flatMap(i => [`\n${i.name}`, i.explanation, `年等效成本：${money(i.convertedAnnual!,r.currency,precision)}`, ...(i.kind === 'oneTime' ? [`一次性原额：${money(i.upfrontOriginal,i.originalCurrency,precision)}`] : []), ...(i.rate ? [`汇率：1 ${i.rate.source} = ${i.rate.rate} ${i.rate.target}；${i.rate.date}；${i.rate.provider}`] : [])]) : []),
    '',r.methodology,`生成时间：${r.generatedAt}`
  ].filter(Boolean).join('\n')).join('\n\n────────────\n\n');
}
export function safeText(text: string): string { return /^[\s]*[=+\-@]/.test(text) || /^[\t\r\n]/.test(text) ? `'${text}` : text; }
const csvCell = (v: string): string => `"${safeText(v).replaceAll('"','""')}"`;
export function csv(results: readonly CalculationResult[]): string {
  exportScope(results);
  const rows = [['方案','项目','计费类型','原币种','汇总币种','日等效成本','周等效成本','月等效成本','年等效成本','一次性原额','汇率','汇率日期','汇率来源','原始参数 JSON','计算依据','生成时间']];
  for (const r of results) for (const i of r.items) rows.push([r.plan.name,i.name,i.kind,i.originalCurrency,r.currency,...PERIODS.map(p=>i.periods![p]),i.upfrontOriginal,i.rate?.rate ?? '1',i.rate?.date ?? '',i.rate?.provider ?? '同币种',JSON.stringify(r.plan.items.find(v=>v.id===i.id)),i.explanation,r.generatedAt]);
  // The summary row ensures even an empty plan has useful standardized output.
  for (const r of results) rows.push([r.plan.name,'方案汇总','summary',r.currency,r.currency,...PERIODS.map(p=>r.total[p]),r.upfront,'','','', '',r.methodology,r.generatedAt]);
  return '\uFEFF'+rows.map(row=>row.map(csvCell).join(',')).join('\r\n')+'\r\n';
}
const projectSchema = z.object({ format: z.literal('ChengSuan'), formatVersion: z.literal(1), exportedAt: z.string().datetime(), plans: z.array(planSchema).min(1).max(1000) });
export function project(results: readonly CalculationResult[]): string { if (!results.length) throw Error('请选择方案'); return JSON.stringify({ format:'ChengSuan',formatVersion:1,exportedAt:new Date().toISOString(),plans:results.map(r=>r.plan) },null,2); }
export function inspectProject(text: string): CostPlan[] {
  let raw: unknown; try { raw=JSON.parse(text); } catch { throw Error('工程文件不是有效 JSON'); }
  if (typeof raw !== 'object' || raw === null || !('formatVersion' in raw) || raw.formatVersion !== 1) throw Error('不支持的工程文件版本');
  return projectSchema.parse(raw).plans;
}
export function cloneImported(plans: readonly CostPlan[], now = new Date().toISOString()): CostPlan[] { return plans.map(p=>planSchema.parse({ ...p,id:crypto.randomUUID(),name:`${p.name}（导入）`,items:p.items.map(i=>({...i,id:crypto.randomUUID()})),createdAt:now,updatedAt:now })); }
export async function xlsx(results: readonly CalculationResult[], precision: number | null): Promise<Uint8Array> {
  exportScope(results);
  const book = new ExcelJS.Workbook(); book.creator='澄算 ChengSuan'; book.created=new Date();
  const overview=book.addWorksheet('方案汇总'); overview.addRow(['方案','说明','币种','日等效成本','周等效成本','月等效成本','年等效成本','一次性费用原额','生成时间']);
  const detail=book.addWorksheet('成本项目'); detail.addRow(['方案','项目','计费类型','原币种','汇总币种','日等效成本','周等效成本','月等效成本','年等效成本','一次性原额','汇率','汇率日期','汇率来源','计算依据']);
  const inputs=book.addWorksheet('原始参数'); inputs.addRow(['方案','项目','原始计费参数（十进制文本保真）']);
  const notes=book.addWorksheet('计算口径'); notes.addRow(['方案','口径说明','生成时间']);
  function amount(value:string, curr:string): string | number {
    const rounded=money(value,curr,precision).split(' ')[1].replaceAll(',','');
    return rounded.replace(/[^\d]/g,'').length<=15 ? Number(rounded) : rounded;
  }
  for(const r of results) {
    overview.addRow([r.plan.name,r.plan.description,r.currency,...PERIODS.map(p=>amount(r.total[p],r.currency)),amount(r.upfront,r.currency),r.generatedAt]);
    for(const i of r.items) { detail.addRow([r.plan.name,i.name,i.kind,i.originalCurrency,r.currency,...PERIODS.map(p=>amount(i.periods![p],r.currency)),amount(i.upfrontOriginal,i.originalCurrency),i.rate?.rate??'1',i.rate?.date??'',i.rate?.provider??'同币种',i.explanation]); inputs.addRow([r.plan.name,i.name,JSON.stringify(r.plan.items.find(v=>v.id===i.id))]); }
    notes.addRow([r.plan.name,r.methodology,r.generatedAt]);
  }
  for(const sheet of book.worksheets) {
    sheet.views=[{ state:'frozen',ySplit:1 }]; sheet.autoFilter={from:{row:1,column:1},to:{row:1,column:sheet.columnCount}};
    sheet.columns.forEach((column,idx)=>{column.width=idx===0?25:idx>11?60:idx===2&&sheet.name==='原始参数'?100:23;});
    sheet.eachRow((row,rowIndex)=>{ row.height=rowIndex===1?30:35; row.eachCell(cell=>{ cell.font={name:'Microsoft YaHei',size:10,color:{argb:rowIndex===1?'FFFFFFFF':'FF151827'}}; cell.alignment={vertical:'middle',wrapText:true}; if(rowIndex===1)cell.fill={type:'pattern',pattern:'solid',fgColor:{argb:'FF5865F2'}}; else if(rowIndex%2===0)cell.fill={type:'pattern',pattern:'solid',fgColor:{argb:'FFF7F8FC'}}; }); });
  }
  for(const [sheet,first,last] of [[overview,4,8],[detail,6,10]] as [ExcelJS.Worksheet,number,number][]) for(let row=2;row<=sheet.rowCount;row++) for(let col=first;col<=last;col++) { const currencyCode=String(sheet.getRow(row).getCell(sheet===overview?3:col===10?4:5).value); const digits=precision??new Intl.NumberFormat('zh-CN',{style:'currency',currency:currencyCode}).resolvedOptions().maximumFractionDigits??2; sheet.getRow(row).getCell(col).numFmt='#,##0'+(digits?'.'+'0'.repeat(digits):''); }
  return new Uint8Array(await book.xlsx.writeBuffer());
}
export const exporters: Record<Exclude<ExportFormat,'text'>,Exporter> = {
  xlsx:{format:'xlsx',render:xlsx}, csv:{format:'csv',render:async r=>new TextEncoder().encode(csv(r))}, chengsuan:{format:'chengsuan',render:async r=>new TextEncoder().encode(project(r))}
};
